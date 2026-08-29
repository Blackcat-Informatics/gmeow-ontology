// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `compile_logic` transform: run the `logic:` compiler inside the build DAG.
//!
//! The pure parse → IR → projection compiler (`gmeow-logic-compile`) is the single
//! producer of every `logic:` information product. Before this stage it ran only
//! behind the `gmeow logic compile` CLI / the PyO3 entry point, so the loss ledger
//! (`projection-report.ttl`) and the compile diagnostics never reached the pipeline
//! rail — they terminated on disk and in conformance fixtures. This stage makes the
//! compiler a first-class DAG node: it parses the canonical logic source, runs every
//! projection back-end once, and emits — as committed artifacts the single-pass
//! update/drift gate owns —
//!
//! * the projection serializations (the canonical RDF 1.2 IR, the OWL DL/EL,
//!   Datalog, N3, gUFO, CLIF, CGIF and XCL projections, and the projection-report loss
//!   ledger), and
//! * the compile diagnostics rendered to the four canonical projections (JSON, SARIF,
//!   HTML, and `gmeow:Finding` N-Quads) — each below-`Exact` projection's structural
//!   drops surfaced as a `logic-compile.lossy-drop` note finding.
//!
//! Downstream, `stage-snapshot` folds the loss ledger into the bundle as its own named
//! graph and unions the compile findings into the diagnostics graph, so a repo-free
//! consumer reads every compiler product without re-running the compiler.
//!
//! ## Engine lock
//!
//! Compilation is pure (parse + projection); it never drives a reasoning engine, so it
//! declares no resource and holds no capability — a parallel-eligible stage with no
//! engine lock.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_errors::Location;
use gmeow_logic_compile::frontend::parse_logic_str;
use gmeow_logic_compile::ir::{LogicProgram, PreservationKind};
use gmeow_logic_compile::openehr_opt::read_all_opt_constraints;
use gmeow_logic_compile::opt_lift::lift_opt_to_validation_shape;
use gmeow_logic_compile::projections::correspondence::{
    CorrespondenceProgram, extract_correspondences, extract_leg_programs, parse_correspondence,
    project_correspondence,
};
use gmeow_logic_compile::projections::correspondence_gates::{
    assert_gates, evaluate_gates, liftability,
};
use gmeow_logic_compile::projections::report::ReportHeader;
use gmeow_logic_compile::projections::{ProjectionResult, compile_program};
use gmeow_logic_compile::relational_core::{
    RelationalCoreProgram, lower_program, project_relational_core,
};
use purrdf::provenance::DatasetProvenance;
use purrdf::{PipelineBundle, RdfDataset, RdfDatasetBuilder, RdfTerm, parse_dataset};
use serde::{Deserialize, Serialize};

use crate::bundle::{PipelineHandle, bundle_from_artifacts_over};
use crate::node::{CachePolicy, Stage, StageInput, StageOutput, StageProduct};
use crate::stages::diag_render::{DiagnosticsPaths, render_diagnostics_artifacts};

/// The single authoritative `logic:` vocabulary source the compiler reads.
pub const SOURCE_PATH: &str = "slices/grounding/logic/module.ttl";

/// The named-graph IRI carrying the canonical RDF-1.2 projection of the compiled
/// [`LogicProgram`] (C6). The compile-logic stage pins its typed
/// [`PipelineHandle::Logic`] handle to THIS graph's canonical digest, and
/// `stage-snapshot` folds the same projection into the bundle under this IRI — so the
/// in-graph carriage and the typed handle are the two faces of one content identity.
pub const GRAPH_LOGIC: &str = gmeow_logic::reasoning_graphs::GRAPH_LOGIC;

/// The named-graph IRI carrying the deterministic RDF projection of the relational-core
/// lowering of the compiled [`LogicProgram`] (C8) — the engine-agnostic
/// Datalog±-with-stratified-negation dialect lowered from the program's Horn `rules`.
/// The compile-logic stage pins its typed [`PipelineHandle::RelationalCore`] handle to
/// THIS graph's canonical digest, and `stage-snapshot` folds the same projection into the
/// bundle under this IRI — so the in-graph carriage and the typed handle are the two faces
/// of one content identity. A downstream consumer reads this LOWERED lane WITHOUT
/// re-lowering. When the full-FOL formula lowering lands, its richer lowering plugs into
/// this SAME lane (same dialect + carried residue).
pub const GRAPH_RELATIONAL_CORE: &str = gmeow_logic::reasoning_graphs::GRAPH_RELATIONAL_CORE;

/// The named-graph IRI carrying the deterministic RDF projection of the compiled
/// [`CorrespondenceProgram`] (C10) — the `logic:Correspondence` carrier lane and
/// the §14 affine-triangle worked transform (`foaf:Person` + `schema:ContactPoint`
/// co-projecting onto `gmeow:contact`). The compile-logic stage pins its typed
/// [`PipelineHandle::Correspondence`] handle to THIS graph's canonical digest, and
/// `stage-snapshot` folds the same projection into the bundle under this IRI — so the
/// in-graph carriage and the typed handle are the two faces of one content identity. The
/// projected alignment surface keeps a caveated overlap at `skos:relatedMatch` (NEVER
/// `skos:exactMatch` / `owl:equivalentClass`); the overclaim gate forbids over-alignment.
pub const GRAPH_CORRESPONDENCE: &str = "https://blackcatinformatics.ca/gmeow/graph/correspondence";

/// Every named graph this stage contributes to the shipped carrier, in fold order.
/// `graph/correspondence` is deliberately carried here: correspondence is first-class
/// shipped ontology content with a digest-pinned typed handle, even though its
/// meta-formula envelope must not enter the object-level reasoning closure.
pub const CARRIER_GRAPHS: [&str; 3] = [GRAPH_LOGIC, GRAPH_RELATIONAL_CORE, GRAPH_CORRESPONDENCE];

/// The object-level named graphs this stage contributes to the reasoned EDB, in fold
/// order. Correspondence is intentionally absent: `logic:Correspondence` relates
/// propositions and target vocabularies at the meta level, so treating endpoint IRIs as
/// object-level axioms would both violate the IR stratification and make external target
/// constructs appear to be authored ontology commitments.
pub const OBJECT_LEVEL_GRAPHS: [&str; 2] = [GRAPH_LOGIC, GRAPH_RELATIONAL_CORE];

/// The object-level graph set as the sorted entity list the typed-dataflow machinery
/// compares (the loader's Rust/RDF bind agreement and the slice-DAG mirror).
pub fn object_level_entity_list() -> Vec<String> {
    let mut entities: Vec<String> = OBJECT_LEVEL_GRAPHS
        .iter()
        .map(|iri| (*iri).to_string())
        .collect();
    entities.sort_unstable();
    entities
}

/// The complete compile-logic carrier graph set as a sorted entity list. Validation
/// depends on the full compiled program, including correspondence, while reasoning uses
/// [`object_level_entity_list`] and therefore cannot consume the meta-level graph.
pub fn carrier_entity_list() -> Vec<String> {
    let mut entities: Vec<String> = CARRIER_GRAPHS
        .iter()
        .map(|iri| (*iri).to_string())
        .collect();
    entities.sort_unstable();
    entities
}

/// Committed OWL 2 DL projection.
pub const OWL_DL_PATH: &str = "generated/owl/gmeow-dl.ttl";
/// Committed OWL 2 EL projection.
pub const OWL_EL_PATH: &str = "generated/owl/gmeow-el.ttl";
/// Committed Datalog projection.
pub const DATALOG_PATH: &str = "generated/datalog/gmeow.dl";
/// Committed N3 rules projection.
pub const N3_PATH: &str = "generated/n3/gmeow.n3";
/// Committed gUFO bridge projection.
pub const GUFO_PATH: &str = "generated/foundation/gufo.ttl";
/// Committed canonical RDF 1.2 IR serialization.
pub const CANONICAL_RDF12_PATH: &str = "generated/logic/gmeow.logic.rdf12.ttl";
/// Committed CLIF (Common Logic Interchange Format) projection: the bidirectional,
/// `PreservationKind::Exact` s-expression FOL dialect.
pub const CLIF_PATH: &str = "generated/cl/gmeow.clif";
/// Committed CGIF (Conceptual Graph Interchange Format) projection: the bidirectional,
/// `PreservationKind::Exact` conceptual-graph FOL dialect (sibling of CLIF, same `generated/cl/`).
pub const CGIF_PATH: &str = "generated/cl/gmeow.cgif";
/// Committed XCL (eXtended Common Logic Markup Language) projection: the bidirectional,
/// `PreservationKind::Exact` XML FOL dialect (sibling of CLIF/CGIF, same `generated/cl/`).
pub const XCL_PATH: &str = "generated/cl/gmeow.xcl";
/// Committed SHACL-AF rule (computation) projection: the canon's derivation rules
/// projected to a `sh:SPARQLRule` surface. Lives under its own `generated/shacl-af/`
/// directory (NOT `generated/shapes/`) so the SHACL constraint validator never ingests
/// these inference rules as no-op constraint shapes.
pub const SHACL_AF_PATH: &str = "generated/shacl-af/gmeow.shacl-af.ttl";

/// The closed-world validation-shape SHACL Core surface: the openEHR OPT/ADL constraint axis
/// lifted to logic:ValidationShape and projected. Lives under generated/shapes/.
pub const VALIDATION_SHAPES_TTL_PATH: &str = "generated/shapes/validation-shapes.ttl";
/// The ShEx projection of the same validation shapes (a strictly narrower surface).
pub const VALIDATION_SHAPES_SHEX_PATH: &str = "generated/shapes/validation-shapes.shex";
/// The procedural-constraint SHACL projection: every closed-world `logic:Constraint`
/// integrity condition projected to a `sh:SPARQLConstraint` NodeShape carrying
/// `logic:formalizes` (the validation twin of the SHACL-AF rule surface). It lives under
/// `generated/shapes/` and is populated from the canonical constraint IR.
pub const PROCEDURAL_CONSTRAINTS_PATH: &str = "generated/shapes/procedural-constraints.ttl";
/// The vendored openEHR OPT the constraint axis lifts (GECCO blood pressure).
pub const OPT_SOURCE_PATH: &str = "validations/openehr-bloodpressure/Blutdruck.opt";
/// A second vendored openEHR OPT — the CaboLabs "Test all datatypes" template, the one real OPT
/// that carries `C_DV_ORDINAL` and `C_DATE_TIME` constraints. Lifting it is what makes the
/// ordinal / datetime constraint families flow slices → gmeow.gts (not just prove in unit tests).
pub const OPT_TEST_DATATYPES_PATH: &str = "validations/openehr-test-datatypes/TestAllDatatypes.opt";
/// The worked-example source authoring the ONLY `a logic:PathShape` individuals in the
/// repo today (design/LOGIC-PATHS.md): `ex:nearbyOrgs` (wildcard, namespace-scoped,
/// bounded depth) and `ex:ancestorsTo3` (named-predicate bounded depth). `SOURCE_PATH`
/// carries only the `logic:PathShape` VOCABULARY (the class + its properties); the
/// authored INSTANCES are a worked example, so they are parsed as a second, independent
/// source and only their [`gmeow_logic_compile::ir::PathShapeIr`]s are folded onto
/// `program` — never their axioms/rules/contracts/formulas/correspondences, which stay
/// scoped to this file and are discarded. Without this, `program.path_shapes` is empty,
/// `paths::project_path_shapes` emits zero per-shape `property-path:<iri>` ledger rows,
/// and the docs term-loss table (`TermLossDigest`) is vacuous on every term.
pub const PATH_SHAPES_EXAMPLE_PATH: &str = "slices/grounding/logic/examples/predicate-paths.ttl";
/// The authored §14 affine-triangle worked example the correspondence lane reads.
///
/// A SCOPED worked-example source (the `PATH_SHAPES_EXAMPLE_PATH` precedent): parsed
/// INDEPENDENTLY of the merged authored corpus and read back via
/// [`gmeow_logic_compile::projections::correspondence::parse_correspondence`] into the
/// one [`CorrespondenceProgram`] the lane projects onto `graph/correspondence`. This is
/// the honest DOGFOODED replacement for the former hardcoded Rust worked example: the
/// affine cell is authored `logic:` TTL, not a `CorrespondenceProgram` literal in code.
pub const CORRESPONDENCE_EXAMPLE_PATH: &str =
    "slices/grounding/logic/examples/affine-correspondence.ttl";
/// The authored goal-directed demonstrator corpus: six `logic:ReasoningProgram`
/// individuals (Peano addition, cons-list membership, three-valued SLG-WFS negation, the
/// positive/negative order-sorted math-subsort pair, and the function-free reachability
/// oracle fixture) that `stage-goal-directed` compiles and evaluates through the native
/// backward engine.
///
/// A SCOPED worked-example source (the `PATH_SHAPES_EXAMPLE_PATH` / `CORRESPONDENCE_EXAMPLE_PATH`
/// precedent): parsed INDEPENDENTLY of the merged authored corpus via `parse_logic_str`, and
/// only its [`gmeow_logic_compile::ir::ReasoningProgramIr`]s are folded onto `program` — never
/// its axioms (e.g. the `ex:one a math:Integer` order-sort typing triple, which
/// `extract_reasoning_programs` already captures into each program's own `constant_sorts`),
/// rules, contracts, or formulas, which stay scoped to this file and are discarded (L3: the
/// cell's clause `Formula`s must never enter `graph/logic` / `graph/relational-core` as
/// top-level rules/formulas).
pub const REASONING_PROGRAMS_EXAMPLE_PATH: &str =
    "slices/grounding/logic/examples/reasoning-programs.ttl";
/// Committed projection-report loss ledger (preservation kinds + lossy drops).
///
/// NOTE: the COMMITTED file at this path is now assembled by `stage-mappings`, which
/// unions the logic projection rows (handed over via [`LOGIC_PROJECTIONS_CHANNEL`]) with
/// the correspondence-calculus loss ledger and serializes the report ONCE. `stage-snapshot`
/// reads it from the mappings product.
pub const PROJECTION_REPORT_PATH: &str = "generated/logic/projection-report.ttl";
/// Committed relational-core dialect projection (C8): the deterministic N-Triples
/// RDF projection of the [`RelationalCoreProgram`] lowered from the program's Horn rules.
/// It is BOTH a committed artifact AND the backing graph the typed RelationalCore handle
/// pins to (the same role the canonical RDF-1.2 projection plays for the Logic handle).
pub const RELATIONAL_CORE_PATH: &str = "generated/logic/gmeow.relational-core.nt";
/// Committed correspondence-lane projection (C10): the deterministic N-Triples RDF
/// projection of the [`CorrespondenceProgram`] (the §14 affine-triangle worked transform).
/// It is BOTH a committed artifact AND the backing graph the typed Correspondence handle
/// pins to (the same role the canonical RDF-1.2 projection plays for the Logic handle).
pub const CORRESPONDENCE_PATH: &str = "generated/logic/gmeow.correspondence.nt";

/// In-memory dataflow channel (the `pipeline/` prefix is never written to disk): the
/// JSON-encoded logic projection rows + report-header counts compile-logic hands to
/// the mappings stage so the latter can assemble the FINAL projection report over the
/// union of the logic rows and the correspondence ledger.
pub const LOGIC_PROJECTIONS_CHANNEL: &str = "pipeline/logic-projections.json";

/// The payload of [`LOGIC_PROJECTIONS_CHANNEL`]: the logic program's projection rows
/// (the eight whole-program targets + the per-shape `property-path:<iri>` rows) and the
/// report-header counts, so the mappings stage can re-serialize the report over the union
/// without re-running the logic compiler.
///
/// Count ownership is split by seam:
/// - `header` carries ONLY the axiom/rule/profile/formula counts compile-logic solely
///   owns (read straight off the compiled program). Its `correspondence_count` /
///   `lawful_uplift_count` / `claimed_uplift_count` fields ride the channel as 0 — they are
///   NOT owned here.
/// - `base_correspondence_count` / `base_lawful_uplift_count` carry the curated affine-gate
///   BASE (the §14 affine-triangle worked-example gate verdicts). mappings composes this
///   base with the external-term up-projection audit to form the committed
///   `correspondenceCount` / `lawfulUpliftCount`. mappings is the SINGLE writer of the final
///   correspondence/uplift/claimed counts (`fold_up_projection_audit`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogicProjectionsChannel {
    /// The report-header counts compile-logic solely owns
    /// (`axiomCount`/`ruleCount`/`profileCount`/`formulaCount`). The correspondence/uplift
    /// count fields of this header ride as 0: mappings owns the final composed values.
    pub header: ReportHeader,
    /// The curated affine-gate BASE for `logic:correspondenceCount`: the number of
    /// correspondences in the §14 affine-triangle gate lane. mappings adds the external-term
    /// audit's `total()` to this base to form the committed count (mappings is the single
    /// writer of the final field).
    pub base_correspondence_count: usize,
    /// The curated affine-gate BASE for `logic:lawfulUpliftCount`: the lawful (round-trip /
    /// mnemomorphism PASS) up-lift count from the affine-triangle gate report. mappings adds
    /// the external-term audit's proved tier to this base to form the committed count
    /// (mappings is the single writer of the final field).
    pub base_lawful_uplift_count: usize,
    /// The logic projection rows that fed the compiler's own (diagnostics-only) report.
    pub projections: Vec<ProjectionResult>,
    /// The compile's single loss store as owned, serializable nodes (the channel is JSON, so the
    /// live ledger cannot cross it). The mappings stage rebuilds the store via
    /// `LossLedger::from_nodes`, unions the correspondence + lang losses in, and reads each row's
    /// residue back through `projection_drops_for` — so the FINAL report's per-target drops flow
    /// from the SAME substrate ledger the producers interned into.
    pub loss_nodes: Vec<gmeow_errors::DiagNode>,
}

/// Committed JSON projection of the compile diagnostics report.
pub const DIAG_JSON_PATH: &str = "generated/diagnostics/logic-compile.json";
/// Committed SARIF projection of the compile diagnostics report.
pub const DIAG_SARIF_PATH: &str = "generated/diagnostics/logic-compile.sarif";
/// Committed HTML projection of the compile diagnostics report.
pub const DIAG_HTML_PATH: &str = "generated/diagnostics/logic-compile.html";
/// Committed `gmeow:Finding` N-Quads projection of the compile diagnostics report.
pub const DIAG_RDF_PATH: &str = "generated/diagnostics/logic-compile.nq";

/// The diagnostics tool/code namespace for this surface.
const TOOL: &str = "logic-compile";

fn stage_err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-compile-logic".to_string(),
        message: message.into(),
    })
}

/// Re-serialize an N-Triples projection as the RDFC-1.0 canonical N-Triples document
/// (blank labels canonicalized, lines bytewise-sorted) so the committed file IS the
/// fold the superset gate reconstructs. RDFC is idempotent, so the round-trip is
/// byte-stable even for the blank-node-bearing relational-core program.
pub(crate) fn canon_fanout_nt(nt: &str) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let ds = parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .map_err(|e| stage_err(format!("parse N-Triples projection: {e}")))?;
    crate::stages::superset::canonical_ntriples(&ds)
        .map_err(|e| stage_err(format!("canonicalize N-Triples projection: {e}")))
}

/// The §14 affine-triangle worked example as the PRODUCTION path derives it: read the
/// authored `CORRESPONDENCE_EXAMPLE_PATH` cell and re-derive its [`CorrespondenceProgram`]
/// via `parse_correspondence`. Test-only helper so every pipeline test exercises the SAME
/// canonical authored source the stage does (the fidelity oracle in `gmeow-logic-compile`
/// proves this equals the `affine_triangle_worked_example` Rust literal byte-for-byte).
#[cfg(test)]
pub(crate) fn affine_worked_example_program() -> CorrespondenceProgram {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(CORRESPONDENCE_EXAMPLE_PATH);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read authored affine correspondence cell {path:?}: {e}"));
    let dataset = parse_dataset(source.as_bytes(), "text/turtle", None)
        .expect("parse authored affine correspondence cell");
    parse_correspondence(&dataset).expect("re-derive authored affine correspondence program")
}

/// The `stage-compile-logic` pipeline stage.
pub struct CompileLogicStage {
    /// The upstream products this stage consumes — `stage-source-load`, off which it reads
    /// the complete [`crate::stages::carrier::GRAPH_LOGIC_COMPILE_INPUTS`] graph (the
    /// lossless merged authored RDF 1.2 corpus its augmentation readers walk).
    consumes: Vec<String>,
    /// The typed dataflow entities: it reads ONLY the `graph/logic-compile-inputs` named
    /// graph of the `stage-source-load` product. The graph retains the whole RDF 1.2
    /// carrier so new ownership/projection readers cannot be starved by an older predicate
    /// filter.
    entities: Vec<(String, Vec<String>)>,
}

impl CompileLogicStage {
    /// Construct the stage. It consumes `stage-source-load`, reading ONLY that product's
    /// complete [`crate::stages::carrier::GRAPH_LOGIC_COMPILE_INPUTS`] named graph for the
    /// five augmentation readers (validation shapes, constraints, correspondences, leg
    /// programs, the diagnostic meta-fold); the canonical `logic:` source and the vendored
    /// OPTs / worked examples it still reads directly from disk (declared via
    /// [`Stage::input_files`]).
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-source-load".to_string()],
            entities: vec![(
                "stage-source-load".to_string(),
                vec![crate::stages::carrier::GRAPH_LOGIC_COMPILE_INPUTS.to_string()],
            )],
        }
    }
}

impl Default for CompileLogicStage {
    fn default() -> Self {
        Self::new()
    }
}

/// Lift EVERY constraint the vendored openEHR OPT walker recognizes — not just the curated
/// blood-pressure quantity pair — to `logic:ValidationShape`s under `base_iri`. `naming` pins
/// named at-codes (e.g. the production systolic at0004 / diastolic at0005 pair) to their
/// established shape/target identity; every other recognized constraint is named from its own
/// enclosing at-code (see [`gmeow_logic_compile::openehr_opt::read_all_opt_constraints`]).
/// Hard-fails on any read/lift error (no optional path).
fn lift_opt_constraints(
    opt_xml: &str,
    base_iri: &str,
    naming: &BTreeMap<String, String>,
) -> Result<Vec<gmeow_logic_compile::ir::ValidationShapeIr>, gmeow_errors::Diag> {
    let constraints = read_all_opt_constraints(opt_xml, base_iri, naming)
        .map_err(|e| stage_err(format!("OPT walk: {e}")))?;
    constraints
        .iter()
        .map(|constraint| {
            lift_opt_to_validation_shape(constraint)
                .map_err(|e| stage_err(format!("OPT lift {}: {e}", constraint.shape_iri)))
        })
        .collect()
}

impl Stage for CompileLogicStage {
    fn id(&self) -> &str {
        "stage-compile-logic"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    /// Typed dataflow (artifact-level): from `stage-source-load` it reads ONLY the
    /// `graph/logic-compile-inputs` named graph (the complete authored RDF 1.2 corpus).
    /// Declaring that entity folds its digest into the compiler's cache key while retaining
    /// every semantic input an evolving reader may consume.
    fn consumed_entities(&self) -> &[(String, Vec<String>)] {
        &self.entities
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
    fn cache_policy(&self) -> CachePolicy {
        // The structural cache stores graph-derived relational/correspondence handles and
        // the complete typed Logic IR. The latter is required because graph/logic is an
        // intentionally lossy governed projection; serving a reverse-parsed shorter
        // program would violate no-optionality.
        CachePolicy::Persistent
    }
    fn impl_version(&self) -> &str {
        // v5: the authored `logic:PathShape` worked-example instances
        // (`PATH_SHAPES_EXAMPLE_PATH`) are now folded into `program.path_shapes`, so
        // `project_path_shapes` emits real per-shape ledger rows (G1: the B2 per-term
        // projection-loss table is no longer vacuous).
        // v6: the affine worked example is now read from `CORRESPONDENCE_EXAMPLE_PATH`
        // (authored `logic:` TTL) via `parse_correspondence`, not a hardcoded Rust literal.
        // v7: the correspondence/uplift BASE rides the channel's `base_*` fields; the report
        // header's count fields ship as 0, and mappings is the single owner of the final counts.
        // v8: the five augmentation readers consume the narrowed source-load
        // `graph/logic-compile-inputs` graph (a SOUND denylist narrowing of the whole
        // authored corpus) instead of re-parsing the corpus from disk; the whole-corpus file
        // list is dropped from `input_files` and freshness rides the typed `consumed_entities`
        // edge.
        // v9: the authored goal-directed demonstrator corpus
        // (`REASONING_PROGRAMS_EXAMPLE_PATH`) is now folded into `program.reasoning_programs`,
        // so `stage-goal-directed` compiles authored `logic:ReasoningProgram`s instead of the
        // hand-interned Rust demonstrator constants.
        // v10: persistence is fail-closed for typed handles. graph/logic deliberately
        // omits source-verbatim IR collections, so a semantically shorter reverse parse
        // is never admitted.
        // v11: the structural cache carries the complete serde LogicProgram payload,
        // authenticates its canonical key, and can therefore persist this exact product.
        // v12: consume the lossless RDF 1.2 source-load carrier. Predicate narrowing was
        // not a stable boundary for the evolving ownership and projection readers.
        "compile-logic.v12-lossless-input"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
        // The compiler parses the canonical `logic:` source, the two vendored OPTs, and the
        // two worked-example cells directly from disk, so those are declared as raw input
        // files for byte-level cache soundness. The WHOLE authored corpus is NO LONGER
        // declared here: the augmentation readers now read the complete
        // `graph/logic-compile-inputs` entity off the `stage-source-load` product
        // (`consumed_entities`), so corpus freshness rides that typed dataflow edge.
        Ok(vec![
            root.join(SOURCE_PATH),
            root.join(OPT_SOURCE_PATH),
            root.join(OPT_TEST_DATATYPES_PATH),
            root.join(PATH_SHAPES_EXAMPLE_PATH),
            root.join(CORRESPONDENCE_EXAMPLE_PATH),
            root.join(REASONING_PROGRAMS_EXAMPLE_PATH),
        ])
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let source = std::fs::read_to_string(input.root.join(SOURCE_PATH))
            .map_err(|e| stage_err(format!("read {SOURCE_PATH}: {e}")))?;
        let (program, mut diagnostics) = parse_logic_str(&source, Some(SOURCE_PATH.to_string()))
            .map_err(|e| stage_err(format!("parse {SOURCE_PATH}: {}", e.0)))?;
        // Constraints axis: lift the vendored openEHR OPTs' constraints to logic:ValidationShapes
        // and attach them, so the SHACL Core + ShEx shape surfaces flow into gmeow.gts as
        // generated projections (DATA FLOWS TO gmeow.gts; maximal dogfooding). A hard fail if
        // either committed OPT is unreadable (no optional path).
        //
        // Two OPTs under distinct base IRIs (no cross-collision): the GECCO blood-pressure
        // template pins its production systolic/diastolic pair by at-code; the CaboLabs
        // "Test all datatypes" template is the one real OPT carrying C_DV_ORDINAL and C_DATE_TIME,
        // so lifting it is what carries the ordinal / datetime families into gmeow.gts.
        const BP_BASE: &str = "https://blackcatinformatics.ca/gmeow/openehr/bloodpressure/";
        let bp_naming = BTreeMap::from([
            ("at0004".to_string(), "Systolic".to_string()),
            ("at0005".to_string(), "Diastolic".to_string()),
        ]);
        let opt_xml = std::fs::read_to_string(input.root.join(OPT_SOURCE_PATH))
            .map_err(|e| stage_err(format!("read {OPT_SOURCE_PATH}: {e}")))?;
        let mut validation_shapes = lift_opt_constraints(&opt_xml, BP_BASE, &bp_naming)?;

        const TEST_DATATYPES_BASE: &str =
            "https://blackcatinformatics.ca/gmeow/openehr/testdatatypes/";
        let td_xml = std::fs::read_to_string(input.root.join(OPT_TEST_DATATYPES_PATH))
            .map_err(|e| stage_err(format!("read {OPT_TEST_DATATYPES_PATH}: {e}")))?;
        validation_shapes.extend(lift_opt_constraints(
            &td_xml,
            TEST_DATATYPES_BASE,
            &BTreeMap::new(),
        )?);
        // Derive closed-world validation shapes from the merged authored ontology's OWL
        // restrictions (someValuesFrom → sh:class), where the DOMAIN restrictions live (the
        // logic: source above carries only the logic: vocabulary). Both the OPT axis and the
        // derived ontology shapes ride into gmeow.gts through the shape surfaces.
        //
        // Read the merged authored corpus as the complete `graph/logic-compile-inputs`
        // entity off the `stage-source-load` product. The producer carries every authored
        // RDF 1.2 quad and statement side table; no predicate-level guess may discard a
        // future ownership or projection input. `project_named_graph` FILTERS to that graph and FLATTENS its quads into the
        // default graph, so the five augmentation readers (all graph-position-agnostic over
        // the default graph) consume it directly. A missing product or an empty projection
        // is a corrupt build — HARD-fail (no-optionality), never a silently-empty corpus.
        let source_load = input.upstream.get("stage-source-load").ok_or_else(|| {
            stage_err("missing stage-source-load product for the graph/logic-compile-inputs corpus")
        })?;
        let ontology = Arc::new(
            source_load
                .bundle()
                .dataset()
                .project_named_graph(crate::stages::carrier::GRAPH_LOGIC_COMPILE_INPUTS),
        );
        if ontology.quad_count() == 0 {
            return Err(stage_err(format!(
                "stage-source-load product carries an empty <{}> graph — the complete \
                 compile-logic input corpus is missing (corrupt upstream product)",
                crate::stages::carrier::GRAPH_LOGIC_COMPILE_INPUTS
            )));
        }
        validation_shapes.extend(
            gmeow_logic_compile::frontend::derive_validation_shapes(ontology.as_ref())
                .map_err(|e| stage_err(format!("derive validation shapes: {e}")))?,
        );
        // Migration-surviving functional-carrier integrity gate. The pre-migration completeness
        // check (`functional_properties_missing_logic_carrier`) became VACUOUS once the
        // `owl:FunctionalProperty` markers were removed — its `declared` set is empty, so it only
        // guards RE-introduction. `functional_carrier_integrity` restores a NON-VACUOUS invariant
        // over the LIVE carrier corpus: it keeps that re-introduction guard AND adds (a) orphan
        // carriers (`logic:characterizes` a non-declared property), (b) duplicate functional
        // carriers, and (c) a positive completeness ledger (the carrier-bearing set must equal the
        // committed frozen `functional_carrier_ledger.txt` — a silent add/drop hard-fails with a
        // diff, forcing a conscious re-bless). HARD FAIL over the merged corpus — never a soft
        // warning; each violation kind is listed distinctly.
        let functional_carrier_violations =
            gmeow_logic_compile::frontend::functional_carrier_integrity(ontology.as_ref());
        if !functional_carrier_violations.is_empty() {
            let count = functional_carrier_violations.len();
            let detail = functional_carrier_violations
                .iter()
                .map(|v| format!("  - {v}"))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(stage_err(format!(
                "functional-carrier integrity: {count} violation{} over the merged corpus \
                 (each a HARD FAIL — a missing/orphan/duplicate carrier or a completeness-ledger \
                 drift):\n{detail}",
                if count == 1 { "" } else { "s" },
            )));
        }
        // Procedural constraints (`logic:Constraint` + the P1–P7 / aggregate sugar) are gathered
        // from the WHOLE merged authored dataset — not only the `logic:` terminal module parsed
        // above — so a constraint may be authored in the slice that OWNS the constrained class (the
        // constraint peer of `derive_validation_shapes`, which already reads the merged ontology).
        // This REPLACES the logic-module-only constraint set the parse above produced (the merged
        // set is a superset, canonicalized by `with_constraints`).
        let (all_constraints, constraint_diags) =
            gmeow_logic_compile::frontend::extract_all_constraints(ontology.as_ref());
        diagnostics.extend(constraint_diags);
        let program = program
            .with_validation_shapes(validation_shapes)
            .with_constraints(all_constraints);

        // Fold in the authored `logic:PathShape` worked-example instances (see
        // `PATH_SHAPES_EXAMPLE_PATH`'s doc comment): parse the example file as an
        // INDEPENDENT logic: source and take ONLY its `path_shapes` — its axioms/
        // rules/contracts/formulas/correspondences stay scoped to that file and are
        // discarded, so the demonstrative org/family-tree facts it also carries never
        // pollute the compiled program's domain axioms. Its diagnostics ARE folded in
        // (never silently dropped), same as every other frontend diagnostic here.
        let path_shapes_source = std::fs::read_to_string(input.root.join(PATH_SHAPES_EXAMPLE_PATH))
            .map_err(|e| stage_err(format!("read {PATH_SHAPES_EXAMPLE_PATH}: {e}")))?;
        let (path_shapes_program, path_shapes_diagnostics) = parse_logic_str(
            &path_shapes_source,
            Some(PATH_SHAPES_EXAMPLE_PATH.to_string()),
        )
        .map_err(|e| stage_err(format!("parse {PATH_SHAPES_EXAMPLE_PATH}: {}", e.0)))?;
        diagnostics.extend(path_shapes_diagnostics);
        let mut path_shapes = program.path_shapes.clone();
        path_shapes.extend(path_shapes_program.path_shapes);
        let program = program.with_path_shapes(path_shapes);

        // Fold in the authored goal-directed demonstrator corpus (see
        // `REASONING_PROGRAMS_EXAMPLE_PATH`'s doc comment): parse the cell as an
        // INDEPENDENT logic: source and take ONLY its `reasoning_programs` — its
        // axioms (the `ex:one a math:Integer` order-sort typing triple is captured
        // by `extract_reasoning_programs` into each program's own `constant_sorts`,
        // not through the plain-axiom lane)/rules/contracts/formulas stay scoped to
        // this file and are discarded, so the demonstrator corpus never pollutes the
        // compiled program's domain axioms or reaches graph/logic /
        // graph/relational-core as top-level rules/formulas (L3). Its diagnostics ARE
        // folded in (never silently dropped), same as every other frontend diagnostic
        // here.
        let reasoning_programs_source =
            std::fs::read_to_string(input.root.join(REASONING_PROGRAMS_EXAMPLE_PATH))
                .map_err(|e| stage_err(format!("read {REASONING_PROGRAMS_EXAMPLE_PATH}: {e}")))?;
        let (reasoning_programs_program, reasoning_programs_diagnostics) = parse_logic_str(
            &reasoning_programs_source,
            Some(REASONING_PROGRAMS_EXAMPLE_PATH.to_string()),
        )
        .map_err(|e| stage_err(format!("parse {REASONING_PROGRAMS_EXAMPLE_PATH}: {}", e.0)))?;
        diagnostics.extend(reasoning_programs_diagnostics);
        if reasoning_programs_program.reasoning_programs.is_empty() {
            return Err(stage_err(format!(
                "{REASONING_PROGRAMS_EXAMPLE_PATH} carries zero logic:ReasoningProgram \
                 individuals — the goal-directed demonstrator corpus is missing (corrupt \
                 worked-example source)"
            )));
        }
        let program =
            program.with_reasoning_programs(reasoning_programs_program.reasoning_programs);

        // The overclaim / rule-safety gate runs inside `compile_program`; a violation
        // is a hard error (fail-closed), never a silently dropped product.
        // Discharge every authored correspondence's lens law by EXECUTION so the five
        // correspondence gates inside `compile_program` read a real per-correspondence verdict.
        // The canonical logic: source authors no `logic:Correspondence` cells today, so this is
        // an empty map (the gates never run) — but authoring one MUST not reach the gates'
        // missing-verdict hard-fail; computing the verdicts here is what guarantees that. (The
        // affine-triangle correspondence lane is discharged + gated separately below.)
        let verdicts = gmeow_logic::correspondence_exec::logic_program_verdicts(&program)
            .map_err(|e| stage_err(format!("discharge correspondence lens laws: {e}")))?;
        let mut arts =
            compile_program(&program, &verdicts).map_err(|e| stage_err(format!("compile: {e}")))?;

        // Correspondence carrier lane (F4): derive the lawful put legs for the §14 affine
        // triangle, run the five gates as a HARD FAIL, and fold the gate-derived liftability
        // statistic into the report header. The affine lane bypasses `compile_program` (whose
        // `program.correspondences` is empty in production), so the gates are RECORDED but
        // never enforced there — this is the one place they are thrown, AND the one place the
        // committed loss ledger learns its `correspondenceCount` / `lawfulUpliftCount` over
        // REAL gate verdicts (the honest replacement for the SSSOM "81% liftable" heuristic).
        // Read the affine cell from its authored `logic:` TTL (the honest dogfooded
        // replacement for the former hardcoded Rust worked example) and re-derive the one
        // `CorrespondenceProgram` via `parse_correspondence` — the EXACT inverse of the
        // `project_correspondence` below, so `graph/correspondence` stays byte-identical.
        // Read/parse failure is a HARD FAIL (no-optionality): a missing or malformed cell
        // is a corrupt build, never a silently-empty lane.
        let correspondence_source =
            std::fs::read_to_string(input.root.join(CORRESPONDENCE_EXAMPLE_PATH))
                .map_err(|e| stage_err(format!("read {CORRESPONDENCE_EXAMPLE_PATH}: {e}")))?;
        let correspondence_dataset =
            parse_dataset(correspondence_source.as_bytes(), "text/turtle", None)
                .map_err(|e| stage_err(format!("parse {CORRESPONDENCE_EXAMPLE_PATH}: {e}")))?;
        let correspondence = parse_correspondence(&correspondence_dataset).map_err(|e| {
            stage_err(format!(
                "re-derive correspondence from {CORRESPONDENCE_EXAMPLE_PATH}: {}",
                e.message()
            ))
        })?;
        let (gated, _gate_outcomes) = correspondence
            .clone()
            .with_derived_puts()
            .map_err(|e| stage_err(format!("derive correspondence put legs: {e}")))?;
        // Discharge the affine triangle's lens laws by EXECUTION (engine-adjacent) and gate on
        // the resulting per-correspondence verdicts — the gates themselves stay execution-free.
        let gate_verdicts = gmeow_logic::correspondence_exec::program_verdicts(&gated);
        let gate_report = evaluate_gates(&gated, &[], &gate_verdicts);
        assert_gates(&gate_report).map_err(|e| stage_err(format!("correspondence gate: {e}")))?;
        let lift = liftability(&gate_report);
        // Count-ownership seam (Seam 1): compile-logic no longer writes the FINAL
        // `correspondence_count` / `lawful_uplift_count` into the report header. It ships the
        // curated affine-gate BASE explicitly on the channel (`base_correspondence_count` /
        // `base_lawful_uplift_count`, populated below from `gated`/`lift`), and mappings'
        // `fold_up_projection_audit` is the SINGLE writer that composes base + external-term
        // audit into the committed counts. Force the header's count fields to 0 so the channel
        // header carries no correspondence/uplift base (`ReportHeader::of_program` seeds
        // `correspondence_count` from `program.correspondences.len()`, which is empty in
        // production but is zeroed here to make the single-owner contract explicit and
        // future-proof).
        arts.report_header.correspondence_count = 0;
        arts.report_header.lawful_uplift_count = 0;
        arts.report_header.claimed_uplift_count = 0;

        // Authored-correspondence enforcement: extract EVERY `a logic:Correspondence`
        // individual from the merged authored surface (the supersession ledger and any
        // other authored crossing), derive its lawful put legs, and run the five gates as
        // a HARD FAIL. This is the throwing seam for authored correspondences — a false
        // Section-Retraction (a claimed recovery whose get leg cannot invert to put ∘ get =
        // id) reds the build here, so a supersession rung can never be an unchecked prose
        // claim. A malformed correspondence cell is surfaced, never silently dropped.
        let (authored_corrs, authored_errors) = extract_correspondences(ontology.as_ref());
        if let Some((iri, msg)) = authored_errors.first() {
            return Err(stage_err(format!(
                "malformed authored logic:Correspondence <{iri}>: {msg}"
            )));
        }
        if !authored_corrs.is_empty() {
            let authored_legs = extract_leg_programs(ontology.as_ref(), &authored_corrs);
            let authored_program =
                CorrespondenceProgram::new(authored_corrs, Vec::new(), PreservationKind::Exact)
                    .with_leg_programs(authored_legs);
            let (authored_gated, _authored_outcomes) = authored_program
                .with_derived_puts()
                .map_err(|e| stage_err(format!("derive authored correspondence put legs: {e}")))?;
            // Discharge the authored correspondences' lens laws by EXECUTION and gate on the
            // resulting per-correspondence verdicts (mirroring the affine lane above) — the
            // law gate now reads real discharge verdicts, so a claimed Section-Retraction whose
            // get leg cannot invert reds the build here rather than passing unverified.
            let authored_verdicts =
                gmeow_logic::correspondence_exec::program_verdicts(&authored_gated);
            let authored_report = evaluate_gates(&authored_gated, &[], &authored_verdicts);
            assert_gates(&authored_report)
                .map_err(|e| stage_err(format!("authored correspondence gate: {e}")))?;
        }

        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        // The nine projection serializations, byte-for-byte as the compiler produced
        // them (RDF targets are reconciled by graph isomorphism, text targets by bytes).
        artifacts.insert(OWL_DL_PATH.to_string(), arts.owl_dl.into_bytes());
        artifacts.insert(OWL_EL_PATH.to_string(), arts.owl_el.into_bytes());
        artifacts.insert(DATALOG_PATH.to_string(), arts.datalog.into_bytes());
        artifacts.insert(N3_PATH.to_string(), arts.n3.into_bytes());
        // gUFO rides as an RDF-fanout named graph: emit EXACTLY the canonical fold
        // (shared prefix authority, no banner) so the superset gate reconstructs it.
        artifacts.insert(
            GUFO_PATH.to_string(),
            purrdf::turtle_normalize::canonical_turtle(
                arts.gufo.as_bytes(),
                &crate::stages::superset::rdf_prefixes(),
            )
            .map(String::into_bytes)
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "stage-compile-logic".to_string(),
                    message: format!("canonicalize gufo.ttl: {e}"),
                })
            })?,
        );
        // Keep the canonical RDF-1.2 projection: it is BOTH a committed artifact AND
        // the backing graph the typed Logic handle (C6) pins to.
        let canonical_rdf12 = arts.canonical_rdf12;
        artifacts.insert(
            CANONICAL_RDF12_PATH.to_string(),
            canonical_rdf12.clone().into_bytes(),
        );
        artifacts.insert(CLIF_PATH.to_string(), arts.clif.into_bytes());
        artifacts.insert(CGIF_PATH.to_string(), arts.cgif.into_bytes());
        artifacts.insert(XCL_PATH.to_string(), arts.xcl.into_bytes());
        // The SHACL-AF rule (computation) surface: the canon's derivation rules projected to
        // sh:SPARQLRule. A byte-decorated text artifact (carries a GENERATED banner), so it
        // rides the generated-fanout archive (REP_GENERATED) as a committed byte projection.
        artifacts.insert(SHACL_AF_PATH.to_string(), arts.shacl_af.into_bytes());
        // The validation-shape surfaces (SHACL Core + ShEx): the OPT/ADL constraint axis
        // projected. Extracted from the ledgered logic_projections so the file bytes and the
        // loss-ledger rows share one source (single-renderer razor).
        let vs_content = |target: &str| {
            arts.logic_projections
                .iter()
                .find(|p| p.target == target)
                .map(|p| p.content.clone())
                .ok_or_else(|| {
                    stage_err(format!(
                        "compile: no '{target}' validation-shape projection produced — the target \
                         string drifted from gmeow-logic-compile's registered projection targets"
                    ))
                })
        };
        let validation_shapes_ttl = vs_content("shacl-core")?;
        purrdf::parse_dataset(validation_shapes_ttl.as_bytes(), "text/turtle", None).map_err(
            |error| {
                stage_err(format!(
                    "compile: emitted validation SHACL is not valid Turtle: {error}"
                ))
            },
        )?;
        artifacts.insert(
            VALIDATION_SHAPES_TTL_PATH.to_string(),
            validation_shapes_ttl.into_bytes(),
        );
        let validation_shapes_shex = vs_content("shex")?;
        purrdf::shex::parse_shexc(&validation_shapes_shex, None).map_err(|error| {
            stage_err(format!(
                "compile: emitted validation ShEx is not well formed: {error}"
            ))
        })?;
        artifacts.insert(
            VALIDATION_SHAPES_SHEX_PATH.to_string(),
            validation_shapes_shex.into_bytes(),
        );
        artifacts.insert(
            PROCEDURAL_CONSTRAINTS_PATH.to_string(),
            vs_content("procedural-constraint")?.into_bytes(),
        );

        // The COMMITTED projection report is no longer emitted here: the loss ledger
        // must carry BOTH the logic projection rows AND the correspondence-calculus
        // rows, and the correspondences are reconstructed only in the mappings stage.
        // Hand the logic rows + header counts to mappings over the in-memory channel;
        // mappings assembles + emits the final `PROJECTION_REPORT_PATH`.
        let channel = LogicProjectionsChannel {
            header: arts.report_header,
            // The curated affine-gate BASE the mappings stage composes with the external-term
            // up-projection audit to form the committed correspondence/uplift counts.
            base_correspondence_count: gated.correspondences.len(),
            base_lawful_uplift_count: lift.lawful,
            projections: arts.logic_projections.clone(),
            loss_nodes: arts.loss.to_nodes(),
        };
        artifacts.insert(
            LOGIC_PROJECTIONS_CHANNEL.to_string(),
            serde_json::to_vec(&channel)
                .map_err(|e| stage_err(format!("encode logic-projections channel: {e}")))?,
        );

        // The relational-core lowering (C8): lower the program's Horn rules into the
        // engine-agnostic Datalog±-with-stratified-negation dialect, then project it into
        // a deterministic N-Triples graph. Keep the projection: it is BOTH a committed
        // artifact AND the backing graph the typed RelationalCore handle pins to. The
        // lowering runs EXACTLY ONCE here; every downstream consumer reads the typed handle
        // (or the folded graph), never re-lowering.
        let relational_core = lower_program(&program);
        let relational_core_nt = project_relational_core(&relational_core);
        artifacts.insert(
            RELATIONAL_CORE_PATH.to_string(),
            canon_fanout_nt(&relational_core_nt)?,
        );

        // The correspondence carrier lane (C10): the §14 affine-triangle worked
        // transform (`foaf:Person` + `schema:ContactPoint` co-projecting onto
        // `gmeow:contact`). Constructed ONCE here, projected ONCE here, then carried BOTH
        // as the typed `PipelineHandle::Correspondence` payload AND its backing
        // `graph/correspondence` projection. The overclaim gate (run at construction +
        // re-asserted below) keeps a caveated affine overlap at `skos:relatedMatch`,
        // never `skos:exactMatch` / `owl:equivalentClass`.
        // The affine triangle was derived + gate-asserted above (the hard-fail + the report
        // header's liftability statistic); project the carrier lane here.
        let correspondence_nt = project_correspondence(&correspondence);
        artifacts.insert(
            CORRESPONDENCE_PATH.to_string(),
            canon_fanout_nt(&correspondence_nt)?,
        );

        // The compile diagnostics: the front-end parse findings (already coded
        // `logic-compile.<code>` by the shared bridge) UNIONED with the loss ledger's
        // OWN witness projection. Rather than hand-build identity-less notes, project the
        // single runtime loss store (`arts.loss`) through `to_finding`: each structural
        // and actual lossy-drop witness surfaces as a finding carrying its stable
        // `finding_iri` / `anchor_iri` and — for an actual drop — the wired antecedent DAG
        // edge (its causing structural-limitation witness) as a structured antecedent +
        // related location. That closing DAG is exactly what the diagnostic meta-fold below
        // joins on to derive `gmeow:findingRootCause` on the SHIPPED bundle (the hand-built
        // notes carried no such identity, so the meta chase derived nothing).
        let mut report = gmeow_logic::logic_diagnostics::diagnostics_report(&diagnostics);
        for finding in arts.loss.project_report(TOOL).findings {
            report.add_finding(finding);
        }
        // Anchor every compiler finding to the real repo-relative source file so
        // SARIF physical locations point to `slices/grounding/logic/module.ttl` rather
        // than falling back to the synthetic `ontology/gmeow.ttl` placeholder.
        // Findings that already carry a physical path (path.is_some()) are left
        // unchanged; logical-only findings (IRI subject) get a prepended physical
        // location so GitHub code-scanning can navigate to the right file.
        for finding in &mut report.findings {
            let has_physical = finding.locations.iter().any(|l| l.path.is_some());
            if !has_physical {
                finding.locations.insert(
                    0,
                    Location::new(Some(SOURCE_PATH.to_string()), None, None, None),
                );
            }
        }
        // Normalize for a deterministic committed artifact (mirrors the PyO3 surface).
        let report = report.normalized();
        // The diagnostic meta-fold: the authored `gmeow:DiagnosticMetaRule` rules (from
        // slices/grounding/logic/module.ttl) + the `gmeow:categoryPolarity` wiring (from
        // slices/core/diagnostics/module.ttl) discovered BY TYPE off the merged authored
        // dataset (`ontology` carries both slices via `load_authored_dataset`). The loss
        // findings above now carry closing antecedent DAGs, so this fold derives the
        // root-cause / cluster / cross-node-glut meta-findings on the SHIPPED bundle.
        let meta = crate::stages::meta_findings::MetaProgram::from_source_dataset(&ontology)
            .map_err(|e| stage_err(format!("diagnostic meta-fold: {e}")))?;
        artifacts.extend(render_diagnostics_artifacts(
            self.id(),
            &report,
            &DiagnosticsPaths {
                json: DIAG_JSON_PATH,
                sarif: DIAG_SARIF_PATH,
                html: DIAG_HTML_PATH,
                rdf: DIAG_RDF_PATH,
            },
            // The logic compiler's findings are Severity::Note lossy-drops (projection
            // loss), never on the gate-fatal up-set, so no gate verdict is derivable.
            None,
            meta.as_ref(),
            // No consumer reads this record back in place of re-running the compiler, so
            // it carries no self-digest (a seal nobody verifies is decoration).
            None,
        )?);

        // The REAL typed Logic handle (C6): carry the compiled program itself
        // on the bundle, pinned to the canonical RDF-1.2 projection of THIS program
        // folded into the `graph/logic` named graph. A downstream consumer takes the
        // typed `Arc<LogicProgram>` and never re-parses the logic graph; on a cache
        // hit the cache re-derives it from the backing graph via `parse_logic_dataset`.
        let bundle = build_logic_bundle(
            program,
            &canonical_rdf12,
            relational_core,
            &relational_core_nt,
            correspondence,
            &correspondence_nt,
            artifacts,
        )?;
        // FORWARD diagnostics fold: the compile report's findings are the SINGLE source
        // of both the shipped `graph/diagnostics` RDF (folded into the bundle above) AND
        // the run-level DiagLedger. Project them once to pre-lowered DiagNodes, carry
        // them on the product's `diagnostics:nodes` blob (so a cache hit re-serves them),
        // and hand them up as `StageOutput.diags` for the scheduler to fold on a fresh run.
        let nodes = crate::stages::diag_render::finding_nodes(&report, self.id());
        let diag_blob = serde_json::to_vec(&nodes)
            .map_err(|e| stage_err(format!("encode diagnostics nodes blob: {e}")))?;
        let bundle = crate::bundle::attach_rep_blob(
            bundle,
            crate::stages::carrier::REP_DIAG_NODES,
            "application/json",
            diag_blob,
        )?;
        Ok(StageOutput {
            product: StageProduct::from_bundle(self.id(), Arc::new(bundle)),
            diags: nodes,
            timings: Vec::new(),
        })
    }
}

/// Assemble the compile-logic product bundle: the named byte-artifact lane riding over
/// a dataset whose `graph/logic` named graph IS the program's canonical RDF-1.2
/// projection, with the typed [`PipelineHandle::Logic`] handle pinned to that graph's
/// canonical digest.
///
/// The handle's payload is the live [`LogicProgram`] (the typed content-addressed IR);
/// its backing graph is the SAME projection `stage-snapshot` folds into `gmeow.gts`, so
/// the in-graph carriage and the handle are pinned to one identity. `pin_handle`
/// HARD-fails on a digest mismatch, so a handle that disagrees with its backing graph
/// can never be attached (no-optionality, fail-closed).
fn build_logic_bundle(
    program: LogicProgram,
    canonical_rdf12_turtle: &str,
    relational_core: RelationalCoreProgram,
    relational_core_nt: &str,
    correspondence: CorrespondenceProgram,
    correspondence_nt: &str,
    artifacts: BTreeMap<String, Vec<u8>>,
) -> Result<PipelineBundle<PipelineHandle>, gmeow_errors::Diag> {
    // All handles ride one bundle: union their backing graphs (each in its own named
    // graph) so each pins to the dataset the bundle carries.
    let logic_dataset = logic_graph_dataset(canonical_rdf12_turtle)?;
    let rc_dataset = relational_core_graph_dataset(relational_core_nt)?;
    let corr_dataset = correspondence_graph_dataset(correspondence_nt)?;
    // The logic-compile diagnostics RDF also rides the carrier, in the shared
    // `graph/diagnostics` named graph, so the presenter unions it with the SHACL
    // diagnostics as a pure keyed fold (PIPELINE_SPINE §4) instead of re-parsing the byte
    // artifact. It is object-level-inert (a Finding graph), so it never reaches the reason
    // EDB (which projects only logic / relational-core). The byte lane is
    // kept for the byte readers.
    let diag_rdf = artifacts.get(DIAG_RDF_PATH).ok_or_else(|| {
        stage_err(format!(
            "build_logic_bundle missing {DIAG_RDF_PATH} artifact"
        ))
    })?;
    let diag_dataset = crate::stages::carrier::parse_into_graph(
        diag_rdf,
        "application/n-quads",
        crate::stages::carrier::GRAPH_DIAGNOSTICS,
    )?;
    let dataset = Arc::new(purrdf::RdfDataset::union(&[
        logic_dataset.as_ref(),
        rc_dataset.as_ref(),
        corr_dataset.as_ref(),
        diag_dataset.as_ref(),
    ]));
    let mut bundle = bundle_from_artifacts_over(dataset, artifacts, DatasetProvenance::new());
    let pinned = bundle.graph_digest(GRAPH_LOGIC);
    bundle
        .pin_handle(
            GRAPH_LOGIC,
            PipelineHandle::Logic(Arc::new(program)),
            pinned,
        )
        .map_err(|e| stage_err(format!("pin Logic handle to <{GRAPH_LOGIC}>: {e}")))?;
    // The REAL typed RelationalCore handle (C8): the typed dialect, pinned to its
    // backing `graph/relational-core` projection. `pin_handle` HARD-fails on a digest
    // mismatch, so a handle that disagrees with its backing graph can never attach.
    let pinned_rc = bundle.graph_digest(GRAPH_RELATIONAL_CORE);
    bundle
        .pin_handle(
            GRAPH_RELATIONAL_CORE,
            PipelineHandle::RelationalCore(Arc::new(relational_core)),
            pinned_rc,
        )
        .map_err(|e| {
            stage_err(format!(
                "pin RelationalCore handle to <{GRAPH_RELATIONAL_CORE}>: {e}"
            ))
        })?;
    // The REAL typed Correspondence handle (C10): the typed correspondence program,
    // pinned to its backing `graph/correspondence` projection. `pin_handle` HARD-fails on
    // a digest mismatch, so a handle that disagrees with its backing graph can never attach.
    let pinned_corr = bundle.graph_digest(GRAPH_CORRESPONDENCE);
    bundle
        .pin_handle(
            GRAPH_CORRESPONDENCE,
            PipelineHandle::Correspondence(Arc::new(correspondence)),
            pinned_corr,
        )
        .map_err(|e| {
            stage_err(format!(
                "pin Correspondence handle to <{GRAPH_CORRESPONDENCE}>: {e}"
            ))
        })?;
    Ok(bundle)
}

/// Parse the correspondence N-Triples projection and route every triple into the
/// `graph/correspondence` named graph of a fresh frozen dataset — the backing graph the
/// typed Correspondence handle pins to and the cache re-derives the program from. Mirrors
/// [`logic_graph_dataset`] so the in-graph carriage and the handle pin to one identity.
fn correspondence_graph_dataset(
    projection_nt: &str,
) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let parsed = parse_dataset(projection_nt.as_bytes(), "application/n-triples", None)
        .map_err(|e| stage_err(format!("parse correspondence projection: {e}")))?;
    let graph = RdfTerm::Iri(GRAPH_CORRESPONDENCE.to_owned());
    let mut builder = RdfDatasetBuilder::new();
    for quad in parsed.owned_quads() {
        let mut routed = quad.clone();
        routed.graph_name = Some(graph.clone());
        builder.push_owned_quad(&routed);
    }
    builder
        .freeze()
        .map_err(|e| stage_err(format!("freeze graph/correspondence dataset: {e}")))
}

/// Parse the relational-core N-Triples projection and route every triple into the
/// `graph/relational-core` named graph of a fresh frozen dataset — the backing graph the
/// typed RelationalCore handle pins to and the cache re-derives the dialect from. Mirrors
/// [`logic_graph_dataset`] so the in-graph carriage and the handle pin to one identity.
fn relational_core_graph_dataset(
    projection_nt: &str,
) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let parsed = parse_dataset(projection_nt.as_bytes(), "application/n-triples", None)
        .map_err(|e| stage_err(format!("parse relational-core projection: {e}")))?;
    let graph = RdfTerm::Iri(GRAPH_RELATIONAL_CORE.to_owned());
    let mut builder = RdfDatasetBuilder::new();
    for quad in parsed.owned_quads() {
        let mut routed = quad.clone();
        routed.graph_name = Some(graph.clone());
        builder.push_owned_quad(&routed);
    }
    builder
        .freeze()
        .map_err(|e| stage_err(format!("freeze graph/relational-core dataset: {e}")))
}

/// Parse the canonical RDF-1.2 projection Turtle and route every triple into the
/// `graph/logic` named graph of a fresh frozen dataset — the backing graph the typed
/// Logic handle pins to. Persistent receipts carry the complete typed program beside
/// this deliberately lossy governed projection.
fn logic_graph_dataset(
    canonical_rdf12_turtle: &str,
) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let parsed = parse_dataset(canonical_rdf12_turtle.as_bytes(), "text/turtle", None)
        .map_err(|e| stage_err(format!("parse canonical rdf12: {e}")))?;
    let graph = RdfTerm::Iri(GRAPH_LOGIC.to_owned());
    let mut builder = RdfDatasetBuilder::new();
    for quad in parsed.owned_quads() {
        let mut routed = quad.clone();
        routed.graph_name = Some(graph.clone());
        builder.push_owned_quad(&routed);
    }
    // The canonical RDF-1.2 projection carries no RDF-1.2 statement-layer side tables
    // (it is a plain RDF-1.1 graph of reifier IRIs), so there are no reifiers/annotations
    // to carry across — the routed quads ARE the whole projection.
    builder
        .freeze()
        .map_err(|e| stage_err(format!("freeze graph/logic dataset: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    use gmeow_logic_compile::frontend::{parse_logic_dataset, parse_logic_str};
    use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom};
    use purrdf::ContentDigest;

    fn compile_logic_fixture() -> StageProduct {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .expect("repository root");
        crate::fixture::stage_fixture(&root, 0, "stage-compile-logic")
            .expect("authenticated compile-logic fixture; tests never produce it")
            .outcome
            .product
    }

    /// A small clean program whose canonical RDF-1.2 projection is an EXACT round-trip
    /// (the documented ExactPreservation case): only graph-derivable constructs —
    /// `rdf:type → logic:Class` axioms (the form the reverse parser re-extracts) — no
    /// modal reifiers, no rule-structural re-emission, no contract facet loss, and a
    /// `None` source (`source_iri` is program provenance the canonical graph does not
    /// carry, so a graph round-trip can only preserve it when it is absent).
    fn clean_program() -> LogicProgram {
        let ax = |s: &str, o: &str| {
            LogicAxiom::new(
                s,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                o,
                false,
                false,
                ContextualScope::default(),
            )
            .expect("valid axiom")
        };
        LogicProgram::new(
            vec![
                ax(
                    "https://blackcatinformatics.ca/gmeow/Animal",
                    "https://blackcatinformatics.ca/logic/Kind",
                ),
                ax(
                    "https://blackcatinformatics.ca/gmeow/Cat",
                    "https://blackcatinformatics.ca/logic/Subkind",
                ),
            ],
            vec![],
            vec![],
            None,
        )
    }

    /// P17 round-trip identity (C6): the canonical RDF-1.2 projection of a
    /// LogicProgram parses back — BOTH via the string reverse parser AND via the
    /// dataset reverse parser the cache uses on a hit — to a canonical-key-EQUAL
    /// program. This is the identity the typed Logic handle relies on: a consumer can
    /// re-derive the program from `graph/logic` and get the same content.
    #[test]
    fn canonical_rdf12_round_trips_to_equal_canonical_key() {
        let program = clean_program();
        let arts = compile_program(&program, &Default::default()).expect("compile clean program");

        // Via the string reverse parser.
        let (rp_str, diags) = parse_logic_str(&arts.canonical_rdf12, program.source_iri.clone())
            .expect("reparse str");
        assert!(
            diags.is_empty(),
            "clean round-trip emits no diagnostics: {diags:?}"
        );
        assert_eq!(
            program.canonical_key(),
            rp_str.canonical_key(),
            "string round-trip must preserve the canonical key"
        );

        // Via the dataset reverse parser the cache hit path uses: project → parse the
        // Turtle into a dataset → reparse the dataset. The handle re-derivation is
        // canonical-key-equal too.
        let ds = parse_dataset(arts.canonical_rdf12.as_bytes(), "text/turtle", None)
            .expect("parse canonical rdf12 to dataset");
        let (rp_ds, _d) =
            parse_logic_dataset(ds.as_ref(), program.source_iri.clone()).expect("reparse dataset");
        assert_eq!(
            program.canonical_key(),
            rp_ds.canonical_key(),
            "dataset round-trip (cache re-derivation) must preserve the canonical key"
        );
    }

    /// The compile-logic stage pins a REAL typed [`PipelineHandle::Logic`] handle to
    /// `graph/logic`, and the handle re-derives (via the SAME reverse parser the cache
    /// uses) to a program whose rules + contracts are isomorphic to the original. The
    /// full real-module canonical key is NOT asserted equal: the canonical RDF-1.2
    /// projection re-emits rules as `logic:rule/...` structural triples that the
    /// reverse parser reads back as BOTH rules and plain axioms (and the module's
    /// `ProbabilisticProfile` contract intentionally drops its `ProbabilityModel` on
    /// projection) — both are documented projection characteristics, not C6 defects.
    /// The rule/contract IR isomorphism is the round-trip identity that holds whole.
    #[test]
    fn stage_pins_logic_handle_re_derivable_to_isomorphic_ir() {
        use crate::bundle::PipelineHandle;
        let product = compile_logic_fixture();
        let bundle = product.bundle();
        let entry = bundle
            .handle(GRAPH_LOGIC)
            .expect("the stage pins a Logic handle to graph/logic");
        // The pin is digest-valid: the pinned digest equals the live graph/logic digest.
        assert_eq!(
            entry.content_digest,
            bundle.graph_digest(GRAPH_LOGIC),
            "the Logic handle is digest-pinned to its backing graph/logic"
        );
        let PipelineHandle::Logic(program) = &entry.payload else {
            panic!("the handle is the Logic arm carrying the typed program");
        };

        // Re-derive the program from the backing graph/logic exactly as the cache does.
        let canonical_ttl = product
            .artifact(CANONICAL_RDF12_PATH)
            .expect("canonical rdf12 artifact");
        let ds = parse_dataset(canonical_ttl, "text/turtle", None).expect("parse backing graph");
        let (re_derived, _d) = parse_logic_dataset(ds.as_ref(), program.source_iri.clone())
            .expect("re-derive program");

        // rules + contracts round-trip isomorphic (whole-program identity).
        let rc = |p: &LogicProgram| {
            LogicProgram::new(
                vec![],
                p.rules.clone(),
                p.contracts.clone(),
                p.source_iri.clone(),
            )
        };
        gmeow_logic_compile::adapter::assert_ir_isomorphic(&rc(program), &rc(&re_derived))
            .expect("the re-derived handle program is rule/contract-isomorphic to the original");
    }

    /// `pin_handle` HARD-fails when the Logic handle's pinned digest disagrees with
    /// its backing graph (no-optionality, fail-closed) — the bundle never carries a
    /// Logic handle that disagrees with `graph/logic`.
    #[test]
    fn pin_logic_handle_hard_fails_on_digest_mismatch() {
        let program = clean_program();
        let arts = compile_program(&program, &Default::default()).expect("compile clean program");
        let dataset = logic_graph_dataset(&arts.canonical_rdf12).expect("graph/logic dataset");
        let mut bundle =
            bundle_from_artifacts_over(dataset, BTreeMap::new(), DatasetProvenance::new());
        // A deliberately WRONG digest (the all-zero digest never equals a real graph).
        let wrong = ContentDigest::of(b"not the graph/logic canonical bytes");
        let err = bundle
            .pin_handle(GRAPH_LOGIC, PipelineHandle::Logic(Arc::new(program)), wrong)
            .expect_err("a mismatched pin must HARD-fail");
        assert!(
            matches!(
                err,
                purrdf::PipelineBundleError::HandleDigestMismatch { .. }
            ),
            "the Logic handle pin must fail closed on a digest mismatch, got {err:?}"
        );
    }

    // ── C8: the relational-core carrier lane ──────────────────────────────

    /// The compile-logic stage pins a REAL typed [`PipelineHandle::RelationalCore`]
    /// handle to `graph/relational-core`, and that handle re-derives (via the SAME
    /// reverse parser the cache uses) from its backing graph to a content-key-EQUAL
    /// dialect. Over main's Horn rules the lowering is `{exact}` (no residue).
    #[test]
    fn stage_pins_relational_core_handle_re_derivable_to_equal_dialect() {
        use gmeow_logic_compile::relational_core::parse_relational_core;
        let product = compile_logic_fixture();
        let bundle = product.bundle();
        let entry = bundle
            .handle(GRAPH_RELATIONAL_CORE)
            .expect("the stage pins a RelationalCore handle to graph/relational-core");
        // The pin is digest-valid: the pinned digest equals the live backing digest.
        assert_eq!(
            entry.content_digest,
            bundle.graph_digest(GRAPH_RELATIONAL_CORE),
            "the RelationalCore handle is digest-pinned to its backing graph/relational-core"
        );
        let PipelineHandle::RelationalCore(program) = &entry.payload else {
            panic!("the handle is the RelationalCore arm carrying the typed dialect");
        };
        // Main's rules are all Horn → the lowering is exact (no carried residue).
        assert!(
            program.residue.is_empty(),
            "main's Horn rule set lowers with no residue; got {:?}",
            program.residue
        );

        // Re-derive the dialect from the backing graph exactly as the cache does, off
        // the committed N-Triples projection artifact.
        let nt = product
            .artifact(RELATIONAL_CORE_PATH)
            .expect("relational-core artifact");
        let ds = parse_dataset(nt, "application/n-triples", None).expect("parse backing graph");
        let re_derived = parse_relational_core(ds.as_ref()).expect("re-derive dialect");
        assert_eq!(
            re_derived.content_key(),
            program.content_key(),
            "the cache re-derivation yields a content-key-equal relational-core dialect"
        );
    }

    /// `pin_handle` HARD-fails when the RelationalCore handle's pinned digest disagrees
    /// with its backing graph (no-optionality, fail-closed).
    #[test]
    fn pin_relational_core_handle_hard_fails_on_digest_mismatch() {
        use gmeow_logic_compile::relational_core::{lower_program, project_relational_core};
        let program = clean_program();
        let lowered = lower_program(&program);
        let nt = project_relational_core(&lowered);
        let dataset = relational_core_graph_dataset(&nt).expect("graph/relational-core dataset");
        let mut bundle =
            bundle_from_artifacts_over(dataset, BTreeMap::new(), DatasetProvenance::new());
        let wrong = ContentDigest::of(b"not the relational-core canonical bytes");
        let err = bundle
            .pin_handle(
                GRAPH_RELATIONAL_CORE,
                PipelineHandle::RelationalCore(Arc::new(lowered)),
                wrong,
            )
            .expect_err("a mismatched pin must HARD-fail");
        assert!(
            matches!(
                err,
                purrdf::PipelineBundleError::HandleDigestMismatch { .. }
            ),
            "the RelationalCore handle pin must fail closed on a digest mismatch, got {err:?}"
        );
    }

    /// No-second-lowering proof: the relational-core lowering runs EXACTLY ONCE (in the
    /// producing stage). A downstream consumer reads the typed handle's already-lowered
    /// dialect — it does NOT call `lower_program` again. This test exercises the consumer
    /// path (`bundle.handle(...).payload`) and asserts it is the typed dialect, equal to
    /// the producer's lowering of the SAME program, without invoking a fresh lowering on
    /// the consumer side.
    #[test]
    fn downstream_consumer_reads_the_handle_without_re_lowering() {
        let product = compile_logic_fixture();
        let bundle = product.bundle();

        // The CONSUMER path: take the typed handle. This is the ONLY way the dialect is
        // obtained downstream — there is no second `lower_program` call here.
        let entry = bundle
            .handle(GRAPH_RELATIONAL_CORE)
            .expect("handle present");
        let PipelineHandle::RelationalCore(consumer_view) = &entry.payload else {
            panic!("consumer reads the RelationalCore handle");
        };

        // It carries a real lowered dialect (facts/rules present), proving the consumer
        // did not have to re-lower to read the rules.
        assert!(
            !consumer_view.facts.is_empty() || !consumer_view.rules.is_empty(),
            "the handle carries the already-lowered dialect (facts and/or rules)"
        );
        // And it is the SAME content as the committed projection the producer emitted —
        // i.e. the producer lowered once and that single result rides both faces.
        let nt = product
            .artifact(RELATIONAL_CORE_PATH)
            .expect("projection artifact");
        let re_derived = gmeow_logic_compile::relational_core::parse_relational_core(
            parse_dataset(nt, "application/n-triples", None)
                .expect("parse")
                .as_ref(),
        )
        .expect("re-derive");
        assert_eq!(
            consumer_view.content_key(),
            re_derived.content_key(),
            "the consumer handle and the folded projection are one content identity"
        );
    }

    // ── C10: the correspondence carrier lane ──────────────────────────────

    /// Correspondence is shipped and digest-pinned, but it is a meta-formula envelope:
    /// target vocabulary IRIs must never be scanned as object-level OWL commitments.
    #[test]
    fn correspondence_is_carried_but_not_reasoned() {
        assert!(
            CARRIER_GRAPHS.contains(&GRAPH_CORRESPONDENCE),
            "the shipped carrier must retain graph/correspondence"
        );
        assert!(
            !OBJECT_LEVEL_GRAPHS.contains(&GRAPH_CORRESPONDENCE),
            "the meta-level correspondence graph must stay outside object-level closure"
        );
        assert!(
            carrier_entity_list().contains(&GRAPH_CORRESPONDENCE.to_string()),
            "validation/cache dataflow must still see the complete compiled carrier"
        );
        assert!(
            !object_level_entity_list().contains(&GRAPH_CORRESPONDENCE.to_string()),
            "reasoning dataflow must not consume correspondence target IRIs"
        );
    }

    /// The compile-logic stage pins a REAL typed [`PipelineHandle::Correspondence`]
    /// handle to `graph/correspondence`, and that handle re-derives (via the SAME reverse
    /// parser the cache uses) from its backing graph to a content-key-EQUAL program. The
    /// load-bearing correctness point is asserted on the committed projection: the §14
    /// affine overlap stays at `skos:relatedMatch`, never `skos:exactMatch` /
    /// `owl:equivalentClass`, and the loss-ledger row is present.
    #[test]
    fn stage_pins_correspondence_handle_re_derivable_with_no_overclaim() {
        use gmeow_logic_compile::projections::correspondence::parse_correspondence;
        let product = compile_logic_fixture();
        let bundle = product.bundle();
        let entry = bundle
            .handle(GRAPH_CORRESPONDENCE)
            .expect("the stage pins a Correspondence handle to graph/correspondence");
        // The pin is digest-valid: the pinned digest equals the live backing digest.
        assert_eq!(
            entry.content_digest,
            bundle.graph_digest(GRAPH_CORRESPONDENCE),
            "the Correspondence handle is digest-pinned to its backing graph/correspondence"
        );
        let PipelineHandle::Correspondence(program) = &entry.payload else {
            panic!("the handle is the Correspondence arm carrying the typed program");
        };

        // The committed projection artifact: the load-bearing alignment correctness point.
        let nt = product
            .artifact(CORRESPONDENCE_PATH)
            .expect("correspondence artifact");
        let nt_str = std::str::from_utf8(nt).expect("utf8");
        // Check the alignment PREDICATE position (`<...#relatedMatch>` as a predicate),
        // not a bare substring — the loss-ledger prose mentions the forbidden predicates
        // by name (that prose is the disclosure, not an emitted alignment edge).
        assert!(
            nt_str.contains("<http://www.w3.org/2004/02/skos/core#relatedMatch>"),
            "the affine overlap stays at skos:relatedMatch:\n{nt_str}"
        );
        assert!(
            !nt_str.contains("<http://www.w3.org/2004/02/skos/core#exactMatch>"),
            "a caveated overlap MUST NOT emit a skos:exactMatch edge:\n{nt_str}"
        );
        assert!(
            !nt_str.contains("<http://www.w3.org/2002/07/owl#equivalentClass>"),
            "a caveated overlap MUST NOT emit an owl:equivalentClass edge:\n{nt_str}"
        );
        assert!(
            nt_str.contains("lossyDrop"),
            "the lane carries an explicit loss-ledger row:\n{nt_str}"
        );

        // Re-derive the program from the backing graph exactly as the cache does.
        let ds = parse_dataset(nt, "application/n-triples", None).expect("parse backing graph");
        let re_derived = parse_correspondence(ds.as_ref()).expect("re-derive program");
        assert_eq!(
            re_derived.content_key(),
            program.content_key(),
            "the cache re-derivation yields a content-key-equal correspondence program"
        );
    }

    /// The overclaim gate is a BUILD FAILURE for an attempt to emit a class equivalence
    /// for the §14 affine/overlaps correspondence the stage carries (the gate fires).
    #[test]
    fn stage_correspondence_overclaim_gate_rejects_equivalence() {
        use gmeow_logic_compile::projections::correspondence::assert_no_overclaim_correspondence;
        let product = compile_logic_fixture();
        let bundle = product.bundle();
        let entry = bundle.handle(GRAPH_CORRESPONDENCE).expect("handle present");
        let PipelineHandle::Correspondence(program) = &entry.payload else {
            panic!("Correspondence arm");
        };
        let correspondence = &program.correspondences[0];
        // Asking for equivalence over this caveated affine overlap is an overclaim → red.
        assert_no_overclaim_correspondence(correspondence, true)
            .expect_err("emitting equivalence for the §14 affine overlap must HARD-fail");
        // The related-match surface (what the lane actually emits) is NOT an overclaim.
        assert_no_overclaim_correspondence(correspondence, false)
            .expect("the related-match surface is not an overclaim");
    }

    /// The five-gate `assert_gates` is now a STAGE hard-fail (not merely recorded): the
    /// lawful affine triangle passes all five, and a constructed RED report errors — the
    /// exact `?` the stage propagates to abort the build around an unlawful correspondence.
    #[test]
    fn stage_asserts_five_correspondence_gates_as_hard_fail() {
        use gmeow_logic_compile::ir::{
            Correspondence, CorrespondenceRelation, MorphismClass, MorphismKind, PreservationKind,
        };
        use gmeow_logic_compile::projections::correspondence_gates::{
            assert_gates, evaluate_gates,
        };

        // The production affine triangle passes the five gates: the wiring will not spuriously
        // fail the build (and the full stage `run` succeeds in the sibling tests).
        let (gated, _) = affine_worked_example_program()
            .with_derived_puts()
            .expect("derive affine put legs");
        let verdicts = gmeow_logic::correspondence_exec::program_verdicts(&gated);
        assert_gates(&evaluate_gates(&gated, &[], &verdicts))
            .expect("the §14 affine triangle is lawful");

        // A bridge view declaring equivalence is an overclaim RED → `assert_gates` errors,
        // which is precisely what the stage propagates as a `gmeow_errors::Diag` build failure.
        let bridge = Correspondence::new(
            "https://gmeow.example/corr/bridge".to_owned(),
            CorrespondenceRelation::Equiv,
            MorphismClass::BridgeView,
            MorphismKind::CommitmentShiftingBridge,
            false,
            None,
            Some("https://gmeow.example/corr/bridgeGet".to_owned()),
            None,
            Vec::new(),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("well-formed bridge correspondence");
        let red =
            CorrespondenceProgram::new(vec![bridge], Vec::new(), PreservationKind::SoundUnder);
        let red_verdicts = gmeow_logic::correspondence_exec::program_verdicts(&red);
        assert_gates(&evaluate_gates(&red, &[], &red_verdicts))
            .expect_err("a bridge-view equivalence overclaim must HARD-fail the build");
    }

    /// Production-path negative control for recovery/leg semantic coupling. The source is
    /// parsed through the real Turtle frontend, receives the production derived put, executes
    /// through `logic_program_verdicts`, compiles through `compile_program`, and reaches the
    /// exact `assert_gates` boundary used by this stage. Changing only the authored get-leg
    /// body while holding the RecoveryCase fixed must therefore red both recovery gates.
    #[test]
    fn stage_recovery_gates_consume_the_resolved_get_leg_body() {
        use gmeow_logic_compile::frontend::Severity;
        use gmeow_logic_compile::projections::correspondence_gates::GateVerdict;

        const SOURCE: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex: <https://example.org/> .

ex:correspondence a logic:Correspondence ;
    logic:correspondenceRelation logic:Subsumes ;
    logic:morphismClass logic:SectionRetraction ;
    logic:morphismKind logic:InstitutionMorphism ;
    logic:mnemomorphic true ;
    logic:getLeg ex:get ;
    logic:recoveryCase ex:case .

ex:get a logic:TransactionProgram ;
    gmeow:path ex:sourceRel .

ex:case a logic:RecoveryCase ;
    logic:recoveryTransform ex:transform .

ex:transform a logic:Formula ;
    logic:quantifiedVariable
        [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "subject" ] ,
        [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termVariable "object" ] ;
    logic:forall [
        a logic:Formula ;
        logic:antecedent [
            a logic:Formula ;
            logic:relation ex:sourceRel ;
            logic:argument
                [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "subject" ] ,
                [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termVariable "object" ]
        ] ;
        logic:consequent [
            a logic:Formula ;
            logic:relation ex:viewRel ;
            logic:argument
                [ a logic:TermCarrier ; logic:termIndex 0 ; logic:termVariable "subject" ] ,
                [ a logic:TermCarrier ; logic:termIndex 1 ; logic:termVariable "object" ]
        ]
    ] .
"#;

        let parse = |source: &str| {
            let (program, diagnostics) = parse_logic_str(
                source,
                Some("https://example.org/recovery-leg-regression".to_owned()),
            )
            .expect("parse recovery correspondence");
            assert!(
                diagnostics
                    .iter()
                    .all(|diagnostic| diagnostic.severity != Severity::Error),
                "unexpected frontend diagnostics: {diagnostics:#?}"
            );
            program
        };

        let baseline = parse(SOURCE);
        let baseline_verdicts = gmeow_logic::correspondence_exec::logic_program_verdicts(&baseline)
            .expect("execute baseline recovery correspondence");
        let baseline_artifacts = compile_program(&baseline, &baseline_verdicts)
            .expect("compile baseline recovery correspondence");
        let baseline_gates = baseline_artifacts
            .correspondence_gates
            .as_ref()
            .expect("baseline correspondence gates");
        assert_gates(baseline_gates).expect("the body-aligned recovery case must pass");

        let mutated_source =
            SOURCE.replacen("gmeow:path ex:sourceRel", "gmeow:path ex:mutatedRel", 1);
        let mutated = parse(&mutated_source);
        assert_eq!(
            baseline.correspondences[0].recovery_cases, mutated.correspondences[0].recovery_cases,
            "the mutation must hold the canonical RecoveryCase fixed"
        );
        let mutated_verdicts = gmeow_logic::correspondence_exec::logic_program_verdicts(&mutated)
            .expect("execute mutated recovery correspondence");
        let mutated_artifacts = compile_program(&mutated, &mutated_verdicts)
            .expect("compile mutated recovery correspondence");
        let mutated_gates = mutated_artifacts
            .correspondence_gates
            .as_ref()
            .expect("mutated correspondence gates");
        let report = &mutated_gates.per_correspondence[0];
        assert!(matches!(report.round_trip, GateVerdict::Red { .. }));
        assert!(matches!(report.mnemomorphism, GateVerdict::Red { .. }));
        assert_gates(mutated_gates).expect_err(
            "changing only the formerly inert LegPath body must hard-fail the production gates",
        );
    }

    /// `pin_handle` HARD-fails when the Correspondence handle's pinned digest disagrees
    /// with its backing graph (no-optionality, fail-closed).
    #[test]
    fn pin_correspondence_handle_hard_fails_on_digest_mismatch() {
        use gmeow_logic_compile::projections::correspondence::project_correspondence;
        let program = affine_worked_example_program();
        let nt = project_correspondence(&program);
        let dataset = correspondence_graph_dataset(&nt).expect("graph/correspondence dataset");
        let mut bundle =
            bundle_from_artifacts_over(dataset, BTreeMap::new(), DatasetProvenance::new());
        let wrong = ContentDigest::of(b"not the correspondence canonical bytes");
        let err = bundle
            .pin_handle(
                GRAPH_CORRESPONDENCE,
                PipelineHandle::Correspondence(Arc::new(program)),
                wrong,
            )
            .expect_err("a mismatched pin must HARD-fail");
        assert!(
            matches!(
                err,
                purrdf::PipelineBundleError::HandleDigestMismatch { .. }
            ),
            "the Correspondence handle pin must fail closed on a digest mismatch, got {err:?}"
        );
    }
}
