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
//!   TBox — the same merged-module graph `make validate` validates, in an
//!   alpha-scoped component isolated from every sibling example. An example
//!   referencing a value individual whose `rdf:type` lives
//!   in its slice's `module.ttl` is therefore CONFORMANT here: that graph carries
//!   the module. Isolation is not a reason to be on the allowlist.
//!   See [`union_with_tbox`].
//! * If an example DOES conform to SHACL but the JSON Schema REJECTS it, that is
//!   a soundness bug in the emitter/projector, surfaced as a test failure with a
//!   readable per-example violation report.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use std::sync::Arc;

use gmeow_validate::instance::{InstanceFormat, validate_instance};
use purrdf::shapes::engine::PreparedValidator;
use purrdf::shapes::report::{Severity, ValidationResult};
use purrdf::shapes::shapes::Shapes;
use purrdf::shapes::term::{NamedNode, Term};
use purrdf::shapes::{instance, json_schema, shape_union};
use rayon::prelude::*;

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
/// ONE cause is represented, and it is not a projection defect:
///
/// * A REAL GAP IN THE EXAMPLE — the scene never binds a property the ontology
///   requires (a P11 reference frame, a `lang:Form`'s sign system, a frame
///   profile's components, a bound variable on a binder expression, a span's
///   offsets). The example is illustrative prose-with-triples, not instance data.
///
/// A shape DEFECT is never a reason to be here. The `gmeow:spanStart`/`gmeow:spanEnd`
/// entries that once were — `gmeow:Chunk-shape` gave both offsets
/// `sh:nodeKind sh:BlankNodeOrIRI` while `slices/core/ai/module.ttl` declares them
/// `owl:DatatypeProperty` with `rdfs:range xsd:nonNegativeInteger`, so the projection
/// rejected a CORRECT integer offset — were removed by fixing the OWL→SHACL projection
/// (`crates/logic-compile/src/frontend.rs`, `classify_on`: an `owl:Thing` filler on a
/// declared datatype property resolves to the DATA-domain top, `sh:nodeKind sh:Literal`),
/// not by allowlisting the examples that the defect rejected.
const NON_CONFORMANT: &[&str] = &[
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
    "slices/extensions/graphrag/examples/lillith-pipeline.ttl", // still non-conformant unioned with the TBox — 1 violation(s): ClassConstraintComponent on embedding-7 (gmeow:embeddingOf, value chunk-7) — the same gmeow:embeddingOf gap purremb-bookshelf carries: gmeow:Chunk reaches gmeow:InformationObject only through the canonical logic:subClassOf edge, which the SHACL engine (an rdfs:subClassOf reader) does not traverse over the AUTHORED TBox
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

/// Private-name prefix for one member of the batched validation union.
///
/// Every example-owned IRI and blank node is alpha-renamed into a disjoint
/// namespace before examples share one immutable validation snapshot. Schema
/// and ontology IRIs remain unchanged, so shape matching is identical while two
/// examples that happen to use the same `ex:item` cannot supply facts to one
/// another.
const EXAMPLE_IRI_SCOPE: &str = "urn:gmeow:example-sweep:";

/// A data-graph assertion on this predicate changes a SHACL-SPARQL target set
/// for every instance of the named open class. Such an example is a target-plan
/// authority, not an ordinary disjoint ABox component, and therefore retains an
/// isolated prepared validator.
const PROFILE_OPEN_VALUE: &str = "https://blackcatinformatics.ca/gmeow/profileOpenValue";

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
/// This is the isolated reference path: it validates an example against the SAME
/// merged-module TBox `make validate` builds, ONE example at a time. The TBox side is
/// [`conformance_support::base_ontology_dataset`] (every `slices/**/module.ttl`,
/// blank-standardized per source, flattened to the default graph). The required
/// sweep obtains the same semantics with [`batched_shacl_violations`]: private
/// names are alpha-scoped before sharing one TBox snapshot. An UNSCOPED corpus
/// merge would let one example silently supply a triple another one is missing.
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
        .map(|result| format_violation(result, &[]))
        .collect()
}

/// Restore one alpha-scoped report field to the lexical form emitted by the
/// one-example reference path.
fn restore_scoped_text(mut text: String, replacements: &[(String, String)]) -> String {
    for (scoped, original) in replacements {
        text = text.replace(scoped, original);
    }
    text
}

/// Render one violation in the stable per-example diagnostic format.
fn format_violation(result: &ValidationResult, replacements: &[(String, String)]) -> String {
    restore_scoped_text(
        format!(
            "{} on {} (path {}, value {}): {}",
            result.source_constraint_component,
            result.focus_node,
            result
                .result_path
                .as_ref()
                .map_or_else(|| "-".to_owned(), ToString::to_string),
            result
                .value
                .as_ref()
                .map_or_else(|| "-".to_owned(), ToString::to_string),
            result.message.clone().unwrap_or_default()
        ),
        replacements,
    )
}

/// Collect every IRI and blank label nested in an RDF 1.2 term.
fn collect_term_names(term: &RdfTerm, iris: &mut BTreeSet<String>, blanks: &mut BTreeSet<String>) {
    match term {
        RdfTerm::Iri(iri) => {
            iris.insert(iri.clone());
        }
        RdfTerm::BlankNode(label) => {
            blanks.insert(label.clone());
        }
        RdfTerm::Literal(literal) => {
            if let Some(datatype) = &literal.datatype {
                iris.insert(datatype.clone());
            }
        }
        RdfTerm::Triple(triple) => {
            collect_term_names(&triple.subject, iris, blanks);
            iris.insert(triple.predicate.clone());
            collect_term_names(&triple.object, iris, blanks);
        }
    }
}

/// Collect the IRI vocabulary and blank labels present in a flat quad set.
fn collect_quad_names(quads: &[RdfQuad]) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut iris = BTreeSet::new();
    let mut blanks = BTreeSet::new();
    for quad in quads {
        collect_term_names(&quad.subject, &mut iris, &mut blanks);
        iris.insert(quad.predicate.clone());
        collect_term_names(&quad.object, &mut iris, &mut blanks);
        if let Some(graph_name) = &quad.graph_name {
            collect_term_names(graph_name, &mut iris, &mut blanks);
        }
    }
    (iris, blanks)
}

/// Alpha-rename one RDF 1.2 term through this example's private-name maps.
fn scope_term(
    term: &mut RdfTerm,
    iris: &BTreeMap<String, String>,
    blanks: &BTreeMap<String, String>,
) {
    match term {
        RdfTerm::Iri(iri) => {
            if let Some(scoped) = iris.get(iri) {
                *iri = scoped.clone();
            }
        }
        RdfTerm::BlankNode(label) => {
            *label = blanks
                .get(label)
                .unwrap_or_else(|| panic!("blank node {label} was not inventoried before scoping"))
                .clone();
        }
        RdfTerm::Literal(_) => {}
        RdfTerm::Triple(triple) => {
            scope_term(&mut triple.subject, iris, blanks);
            if let Some(scoped) = iris.get(&triple.predicate) {
                triple.predicate = scoped.clone();
            }
            scope_term(&mut triple.object, iris, blanks);
        }
    }
}

/// One example projected into a private alpha-renamed component of the shared
/// validation dataset.
struct ScopedProjection {
    quads: Vec<RdfQuad>,
    focus: Vec<Term>,
    /// Scoped lexical token -> the lexical token the isolated reference emits.
    replacements: Vec<(String, String)>,
}

/// Project and alpha-scope one example while leaving the ontology/shape
/// vocabulary untouched.
fn scoped_projection(
    example: &RdfDataset,
    public_iris: &BTreeSet<String>,
    example_index: usize,
) -> ScopedProjection {
    let projected =
        purrdf::shapes::engine::project_dataset(example).expect("SHACL projection of an example");
    let mut quads = flat_rdf_quads_from_dataset(&projected);
    let (mut private_iris, blank_labels) = collect_quad_names(&quads);
    private_iris.retain(|iri| !public_iris.contains(iri));

    let iri_scope: BTreeMap<String, String> = private_iris
        .into_iter()
        .enumerate()
        .map(|(ordinal, iri)| {
            (
                iri,
                format!("{EXAMPLE_IRI_SCOPE}{example_index}:iri:{ordinal}"),
            )
        })
        .collect();
    let blank_scope: BTreeMap<String, String> = blank_labels
        .into_iter()
        .enumerate()
        .map(|(ordinal, label)| {
            (
                label,
                format!("{EXAMPLE_BLANK_SCOPE}{example_index}-{ordinal}"),
            )
        })
        .collect();

    for quad in &mut quads {
        scope_term(&mut quad.subject, &iri_scope, &blank_scope);
        if let Some(scoped) = iri_scope.get(&quad.predicate) {
            quad.predicate = scoped.clone();
        }
        scope_term(&mut quad.object, &iri_scope, &blank_scope);
        if let Some(graph_name) = &mut quad.graph_name {
            scope_term(graph_name, &iri_scope, &blank_scope);
        }
    }

    let mut focus_iris = BTreeSet::new();
    let mut focus_blanks = BTreeSet::new();
    for quad in &quads {
        for term in [&quad.subject, &quad.object] {
            match term {
                RdfTerm::Iri(iri) => {
                    focus_iris.insert(iri.clone());
                }
                RdfTerm::BlankNode(label) => {
                    focus_blanks.insert(label.clone());
                }
                RdfTerm::Literal(_) | RdfTerm::Triple(_) => {}
            }
        }
    }
    let focus = focus_iris
        .into_iter()
        .map(|iri| Term::NamedNode(NamedNode::new_unchecked(iri)))
        .chain(focus_blanks.into_iter().map(Term::BlankNode))
        .collect();

    let mut replacements: Vec<(String, String)> = iri_scope
        .into_iter()
        .map(|(original, scoped)| (scoped, original))
        .chain(
            blank_scope
                .into_iter()
                .map(|(original, scoped)| (scoped, format!("{EXAMPLE_BLANK_SCOPE}{original}"))),
        )
        .collect();
    // Replace the longest lexical tokens first so one generated token can never
    // be consumed as a prefix of another.
    replacements.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));

    ScopedProjection {
        quads,
        focus,
        replacements,
    }
}

/// Deterministic causal-work counters for the batched SHACL phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShaclWork {
    projected_examples: usize,
    validator_preparations: usize,
    tbox_rows_materialized: usize,
}

/// Per-example violation reports from one prepared validator over an
/// alpha-scoped union.
struct BatchedShacl {
    violations: Vec<Vec<String>>,
    work: ShaclWork,
}

/// Validate every example with one TBox materialization and one prepared
/// validator while retaining per-example identity and report text.
fn alpha_scoped_shacl_violations(
    examples: &[Arc<RdfDataset>],
    tbox: &[RdfQuad],
    shapes: &Arc<Shapes>,
    public_iris: &BTreeSet<String>,
) -> BatchedShacl {
    assert!(!examples.is_empty(), "batched SHACL requires an example");

    let projections: Vec<ScopedProjection> = examples
        .iter()
        .enumerate()
        .map(|(index, example)| scoped_projection(example, public_iris, index))
        .collect();
    let example_rows_materialized: usize = projections
        .iter()
        .map(|projection| projection.quads.len())
        .sum();

    let mut quads = Vec::with_capacity(tbox.len() + example_rows_materialized);
    quads.extend_from_slice(tbox);
    for projection in &projections {
        quads.extend_from_slice(&projection.quads);
    }
    let merged =
        flat_dataset_from_quads(&quads).expect("the TBox plus alpha-scoped examples must freeze");

    let mut focus_owners: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut all_focus = Vec::new();
    for (index, projection) in projections.iter().enumerate() {
        for focus in &projection.focus {
            focus_owners
                .entry(focus.to_string())
                .or_default()
                .push(index);
            all_focus.push(focus.clone());
        }
    }

    let validator = PreparedValidator::from_projected_dataset(merged, Arc::clone(shapes))
        .expect("prepare the TBox plus alpha-scoped examples for validation");
    let report = validator
        .validate_focus_nodes(&all_focus)
        .expect("validation over a frozen dataset is infallible");

    let mut violations = vec![Vec::new(); examples.len()];
    for result in report
        .results
        .iter()
        .filter(|result| matches!(result.severity, Severity::Violation))
    {
        let focus = result.focus_node.to_string();
        let owners = focus_owners.get(&focus).unwrap_or_else(|| {
            panic!("validation returned focus node {focus} outside the supplied focus inventory")
        });
        for &owner in owners {
            violations[owner].push(format_violation(result, &projections[owner].replacements));
        }
    }
    for per_example in &mut violations {
        per_example.sort();
    }

    BatchedShacl {
        violations,
        work: ShaclWork {
            projected_examples: examples.len(),
            validator_preparations: 1,
            tbox_rows_materialized: tbox.len(),
        },
    }
}

/// Whether an example authors data that changes a global SHACL-SPARQL target
/// plan rather than only the neighborhood of its own focus nodes.
fn changes_global_target_authority(example: &RdfDataset) -> bool {
    example
        .owned_quads()
        .any(|quad| quad.predicate == PROFILE_OPEN_VALUE)
        || example
            .owned_annotations()
            .any(|annotation| annotation.predicate == PROFILE_OPEN_VALUE)
}

/// Deterministically balance examples across the CPU width exposed to this
/// process, using source quad count as the stable cost proxy.
fn balanced_batches(examples: &[Arc<RdfDataset>], indices: &[usize]) -> Vec<Vec<usize>> {
    if indices.is_empty() {
        return Vec::new();
    }
    let width = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
        .min(indices.len());
    let mut batches = vec![Vec::new(); width];
    let mut loads = vec![0usize; width];
    let mut ordered = indices.to_vec();
    ordered.sort_by_key(|&index| (Reverse(examples[index].quad_count()), index));
    for index in ordered {
        let bucket = loads
            .iter()
            .enumerate()
            .min_by_key(|&(bucket, load)| (*load, bucket))
            .map(|(bucket, _)| bucket)
            .expect("a non-empty batch inventory has a worker bucket");
        batches[bucket].push(index);
        loads[bucket] = loads[bucket]
            .checked_add(examples[index].quad_count())
            .expect("example batch cost must fit usize");
    }
    for batch in &mut batches {
        batch.sort_unstable();
    }
    batches
}

/// Validate ordinary disjoint ABox examples in natural-width alpha-scoped
/// partitions and keep target-plan authorities in isolated snapshots.
///
/// The distinction is structural, not a path allowlist: `profileOpenValue`
/// occurs in the shipped `ProfileOpenValueUseConstraint` SPARQL target query.
/// Adding one such triple changes which nodes are targets across the complete
/// graph, so placing that authority beside unrelated examples would alter their
/// semantics even after all private node names were scoped.
fn batched_shacl_violations(
    examples: &[Arc<RdfDataset>],
    tbox: &[RdfQuad],
    shapes: &Arc<Shapes>,
    public_iris: &BTreeSet<String>,
) -> BatchedShacl {
    assert!(!examples.is_empty(), "batched SHACL requires an example");

    let (isolated, batched): (Vec<usize>, Vec<usize>) = (0..examples.len())
        .partition(|&index| changes_global_target_authority(examples[index].as_ref()));
    let mut violations = vec![Vec::new(); examples.len()];
    let mut work = ShaclWork {
        projected_examples: 0,
        validator_preparations: 0,
        tbox_rows_materialized: 0,
    };

    let batches = balanced_batches(examples, &batched);
    let mut scheduled = isolated.clone();
    scheduled.extend(batches.iter().flatten().copied());
    scheduled.sort_unstable();
    assert_eq!(
        scheduled,
        (0..examples.len()).collect::<Vec<_>>(),
        "isolated plus alpha-scoped partitions must cover every example exactly once"
    );
    let batch_results: Vec<(Vec<usize>, BatchedShacl)> = batches
        .par_iter()
        .map(|indices| {
            let batch_examples: Vec<Arc<RdfDataset>> = indices
                .iter()
                .map(|&index| Arc::clone(&examples[index]))
                .collect();
            (
                indices.clone(),
                alpha_scoped_shacl_violations(&batch_examples, tbox, shapes, public_iris),
            )
        })
        .collect();
    for (indices, batch) in batch_results {
        for (&source, result) in indices.iter().zip(batch.violations) {
            violations[source] = result;
        }
        work.projected_examples += batch.work.projected_examples;
        work.validator_preparations += batch.work.validator_preparations;
        work.tbox_rows_materialized += batch.work.tbox_rows_materialized;
    }

    let isolated_results: Vec<(usize, Vec<String>)> = isolated
        .par_iter()
        .map(|&index| {
            let mut result = shacl_violations(&examples[index], tbox, shapes);
            result.sort();
            (index, result)
        })
        .collect();
    for (index, result) in isolated_results {
        violations[index] = result;
    }
    work.projected_examples += isolated.len();
    work.validator_preparations += isolated.len();
    work.tbox_rows_materialized += tbox.len() * isolated.len();

    BatchedShacl { violations, work }
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

/// One parsed example and the allowlist disposition derived from its stable
/// repository-relative identity.
struct ExampleCase {
    relpath: String,
    store: Arc<RdfDataset>,
    excluded: bool,
}

/// Finish one example after the shared SHACL phase: when it is in scope, project
/// its own data graph to the closed-world JSON Schema surface.
fn sweep_one(case: &ExampleCase, shacl_violations: Vec<String>, schema_bytes: &[u8]) -> Outcome {
    // Project the EXAMPLE ALONE to JSON-LD and validate against the
    //     closed-world schema: the schema's claim is about the instance document
    //     an emitter ships, which carries the example's data and nothing else.
    let schema_violations = if case.excluded {
        Vec::new()
    } else {
        let instance_value = instance::project_graph(&case.store, &gmeow_namespaces());
        let instance_bytes = serde_json::to_vec(&instance_value).expect("serialize instance");
        validate_instance(&instance_bytes, InstanceFormat::Json, schema_bytes)
            .unwrap_or_else(|e| panic!("validate_instance hard error for {}: {e}", case.relpath))
    };

    Outcome {
        relpath: case.relpath.clone(),
        shacl_violations,
        excluded: case.excluded,
        schema_violations,
    }
}

#[test]
fn example_corpus_validates_against_closed_world_schema() {
    let repo = repo_root();

    // The merged shape union + the JSON Schema derived from those same shapes.
    let (shapes_store, shapes) = shape_union::load_shapes(&repo).expect("load merged SHACL shapes");
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

    // Parse each source exactly once. Rayon preserves the sorted input order for
    // this indexed collect; the later explicit sort keeps report order independent
    // of that implementation detail.
    let cases: Vec<ExampleCase> = examples
        .par_iter()
        .map(|path| {
            let relpath = rel(&repo, path);
            ExampleCase {
                excluded: non_conformant.contains(relpath.as_str()),
                relpath,
                store: load_data_graph(path),
            }
        })
        .collect();

    // The TBox and shape graph define the public vocabulary that must keep its
    // identity. Every other IRI and every blank node is example-owned and is
    // alpha-scoped before examples enter natural-width immutable partitions. This
    // replaces one TBox copy + validator preparation per example with one per
    // semantic partition, while retaining one focus/report partition per example.
    let (mut public_iris, _) = collect_quad_names(&tbox);
    let shape_quads = flat_rdf_quads_from_dataset(&shapes_store);
    let (shape_iris, _) = collect_quad_names(&shape_quads);
    public_iris.extend(shape_iris);
    let stores: Vec<Arc<RdfDataset>> = cases.iter().map(|case| Arc::clone(&case.store)).collect();
    let isolated_count = stores
        .iter()
        .filter(|store| changes_global_target_authority(store))
        .count();
    let batchable: Vec<usize> = (0..stores.len())
        .filter(|&index| !changes_global_target_authority(&stores[index]))
        .collect();
    let batch_preparations = balanced_batches(&stores, &batchable).len();
    let batched = batched_shacl_violations(&stores, &tbox, &shapes, &public_iris);
    assert_eq!(
        batched.work,
        ShaclWork {
            projected_examples: examples.len(),
            validator_preparations: isolated_count + batch_preparations,
            tbox_rows_materialized: tbox.len() * (isolated_count + batch_preparations),
        },
        "the corpus sweep must project each example once and prepare/materialize the TBox once per semantic partition"
    );
    eprintln!(
        "example-sweep-work projected_examples={} validator_preparations={} tbox_rows_materialized={}",
        batched.work.projected_examples,
        batched.work.validator_preparations,
        batched.work.tbox_rows_materialized,
    );

    let mut outcomes: Vec<Outcome> = cases
        .par_iter()
        .zip(batched.violations.into_par_iter())
        .map(|(case, shacl_violations)| sweep_one(case, shacl_violations, schema_bytes))
        .collect();
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

#[test]
fn batched_union_keeps_same_named_examples_isolated() {
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const PAIR: &str = "https://example.test/Pair";
    const LEFT: &str = "https://example.test/left";
    const RIGHT: &str = "https://example.test/right";

    let shapes = Arc::new(
        purrdf::shapes::engine::parse_shapes(
            r#"
            @prefix sh: <http://www.w3.org/ns/shacl#> .
            @prefix ex: <https://example.test/> .

            ex:PairShape a sh:NodeShape ;
                sh:targetClass ex:Pair ;
                sh:property [ sh:path ex:left ; sh:minCount 1 ] ;
                sh:property [ sh:path ex:right ; sh:minCount 1 ] .
            "#,
        )
        .expect("parse isolation shapes"),
    );
    let examples = [
        parse_dataset(
            br#"
            @prefix ex: <https://example.test/> .
            ex:item a ex:Pair ; ex:left "from-left" .
            "#,
            "text/turtle",
            None,
        )
        .expect("parse left example"),
        parse_dataset(
            br#"
            @prefix ex: <https://example.test/> .
            ex:item a ex:Pair ; ex:right "from-right" .
            "#,
            "text/turtle",
            None,
        )
        .expect("parse right example"),
    ];
    let public_iris = [RDF_TYPE, PAIR, LEFT, RIGHT]
        .into_iter()
        .map(ToOwned::to_owned)
        .collect();

    let batched = alpha_scoped_shacl_violations(&examples, &[], &shapes, &public_iris);
    assert_eq!(batched.work.validator_preparations, 1);
    assert_eq!(batched.violations.len(), 2);
    assert_eq!(
        batched.violations[0].len(),
        1,
        "the left-only example must still miss right: {:?}",
        batched.violations[0]
    );
    assert_eq!(
        batched.violations[1].len(),
        1,
        "the right-only example must still miss left: {:?}",
        batched.violations[1]
    );
    assert!(
        batched
            .violations
            .iter()
            .flatten()
            .all(|violation| violation.contains("MinCountConstraintComponent")),
        "both isolated components must retain their own missing-property result: {:?}",
        batched.violations
    );
}

/// Exhaustive equivalence proof for the optimized alpha-scoped corpus union.
///
/// The required sweep above pins the complete conformant/non-conformant corpus
/// partition on every commit, while this maintainer lane recomputes all
/// independent TBox unions and compares every violation byte-for-byte.
#[test]
fn batched_shacl_matches_isolated_reference_heavy_offgate() {
    let repo = repo_root();
    let (shapes_store, shapes) = shape_union::load_shapes(&repo).expect("load merged SHACL shapes");
    let shapes = Arc::new(shapes);
    let tbox = flat_rdf_quads_from_dataset(
        &purrdf::shapes::engine::project_dataset(conformance_support::base_ontology_dataset())
            .expect("SHACL projection of the merged module TBox"),
    );
    let paths = example_files(&repo);
    assert!(
        !paths.is_empty(),
        "the isolated parity corpus must be non-empty"
    );
    let examples: Vec<Arc<RdfDataset>> =
        paths.par_iter().map(|path| load_data_graph(path)).collect();

    let (mut public_iris, _) = collect_quad_names(&tbox);
    let shape_quads = flat_rdf_quads_from_dataset(&shapes_store);
    let (shape_iris, _) = collect_quad_names(&shape_quads);
    public_iris.extend(shape_iris);

    let batched = batched_shacl_violations(&examples, &tbox, &shapes, &public_iris);
    let isolated: Vec<Vec<String>> = examples
        .par_iter()
        .map(|example| {
            let mut violations = shacl_violations(example, &tbox, &shapes);
            violations.sort();
            violations
        })
        .collect();
    for ((path, batched), isolated) in paths.iter().zip(&batched.violations).zip(&isolated) {
        assert_eq!(
            batched,
            isolated,
            "alpha-scoped batch report drifted for {}",
            rel(&repo, path)
        );
    }
}
