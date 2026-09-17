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
//! Compilation includes native correspondence-law evaluation and source-presentation
//! checking. Those operations own their invocation-local state and share immutable
//! source publications; there is no external reasoner or process-global engine state
//! to lock. The stage therefore remains eligible for parallel scheduling.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_logic_compile::frontend::{
    CompiledTheory, OwnerDisposition, OwnerFamily, parse_logic_str,
};
use gmeow_logic_compile::ir::LogicProgram;
use gmeow_logic_compile::openehr_opt::read_all_opt_constraints;
use gmeow_logic_compile::opt_lift::lift_opt_to_validation_shape;
use gmeow_logic_compile::projections::correspondence::{
    CorrespondenceProgram, parse_correspondence, project_correspondence_dataset,
};
use gmeow_logic_compile::projections::correspondence_gates::{
    assert_gates, evaluate_gates, liftability,
};
use gmeow_logic_compile::projections::report::ProjectionReportRow;
use gmeow_logic_compile::projections::{CompiledArtifacts, compile_program};
use gmeow_logic_compile::relational_core::{
    RelationalCoreProgram, lower_program_with_formulas, project_relational_core_dataset,
};
use purrdf::provenance::DatasetProvenance;
use purrdf::{PipelineBundle, RdfDataset, parse_dataset};

use crate::bundle::{
    CompiledLogicPublication, LogicReportInputs, PipelineHandle, bundle_from_artifacts_over,
};
use crate::node::{CachePolicy, Stage, StageInput, StageOutput, StageProduct};
use crate::stages::diag_render::{
    DiagnosticsPaths, RenderedDiagnostics, render_diagnostics_artifacts,
};

mod abduction;
mod correspondence_roundtrip;
mod formula_fixtures;
mod native_carrier;
mod projection_fixtures;
mod roundtrip;
mod validation_fixtures;
mod vocabulary_fixtures;

/// The grounding vocabulary document within the complete admitted source catalog.
pub const SOURCE_PATH: &str = "slices/grounding/logic/module.ttl";
/// Canonical provenance identity for the standalone grounding-logic module.
///
/// Every producer consumer must use this exact identity when asking the shared
/// source catalog for the compiled document. A second spelling creates a second
/// lowering of the same large module and leaves a serial compiler tail after the
/// parallel conformance observations have completed.
pub const SOURCE_IRI: &str = gmeow_logic::operator_rules::OPERATOR_SOURCE_IRI;

/// The named-graph IRI carrying the canonical RDF-1.2 projection of the compiled
/// [`LogicProgram`] (C6). The compile-logic stage pins its typed
/// [`PipelineHandle::CompiledLogic`] handle to THIS graph's canonical digest, and
/// `stage-snapshot` folds the same projection into the bundle under this IRI — so the
/// in-graph carriage and the typed handle are the two faces of one content identity.
pub const GRAPH_LOGIC: &str = gmeow_logic::reasoning_graphs::GRAPH_LOGIC;

/// The named-graph IRI carrying the deterministic RDF projection of the relational-core
/// lowering of the compiled [`LogicProgram`] (C8) — the engine-agnostic
/// Datalog±-with-stratified-negation dialect lowered from the program's Horn rules
/// and the supported Formula fragment, with explicit residue for unlowered formulas.
/// The compile-logic stage pins its typed [`PipelineHandle::RelationalCore`] handle to
/// THIS graph's canonical digest, and `stage-snapshot` folds the same projection into the
/// bundle under this IRI — so the in-graph carriage and the typed handle are the two faces
/// of one content identity. A downstream consumer reads this lowered lane and its
/// residue without repeating lowering.
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
/// Certified physical plans for every executable authored correspondence
/// composition. The typed program remains the semantic authority; this artifact
/// records deterministic native plan selection and its independently checkable
/// scope.
pub const CORRESPONDENCE_PLANS_PATH: &str = "generated/logic/correspondence-physical-plans.json";
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
/// The authored executable correspondence worked program the carrier lane reads.
///
/// A SCOPED worked-example source (the `PATH_SHAPES_EXAMPLE_PATH` precedent): parsed
/// INDEPENDENTLY of the merged authored corpus and read back via
/// [`gmeow_logic_compile::projections::correspondence::parse_correspondence`] into the
/// selected [`CorrespondenceProgram`] merged into the complete source program. It
/// carries the §14 affine cell and the executable blood-pressure path compositions;
/// neither is a hardcoded Rust `CorrespondenceProgram` literal.
pub const CORRESPONDENCE_EXAMPLE_PATH: &str =
    "slices/grounding/logic/examples/correspondence-program.ttl";
/// The process-axis worked source: a fixed-count RCHOPS21 `logic:Plan` carrying
/// guarded branches, tracked-state freshness, nondeterministic outcomes,
/// compensation and a conditional addition. It is projected by the same native
/// correspondence execution layer, never treated as an observed run.
pub const RCHOPS21_PLAN_SOURCE_PATH: &str =
    "docs/APPLIED_CATEGORY_THEORY/fixtures/rchops21.plan.ttl";
/// A descriptive process record whose occurrences carry the two in-band
/// plan/schema witnesses required for honest planned-skeleton recovery.
pub const RCHOPS21_OBSERVED_SOURCE_PATH: &str =
    "docs/APPLIED_CATEGORY_THEORY/fixtures/rchops21.observed.ttl";
/// Certified bounded projection of [`RCHOPS21_PLAN_SOURCE_PATH`].
pub const RCHOPS21_PLAN_PROJECTION_PATH: &str = "generated/logic/rchops21-plan-projection.json";
/// Certified planned-skeleton recovery from [`RCHOPS21_OBSERVED_SOURCE_PATH`].
pub const RCHOPS21_PLAN_RECOVERY_PATH: &str = "generated/logic/rchops21-plan-recovery.json";
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
/// unions the logic projection rows (handed over via [`LogicReportInputs`]) with
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

/// Lower a detached compiler observation at its artifact boundary. Operational
/// functions propagate live diagnostics; the artifact retains the typed record.
fn observed<T>(result: gmeow_errors::Result<T>) -> Result<T, gmeow_errors::RecordedDiag> {
    result.map_err(record_failure)
}

fn record_failure(error: impl Into<gmeow_errors::Diag>) -> gmeow_errors::RecordedDiag {
    gmeow_errors::DiagLedger::new().record(
        error.into(),
        gmeow_errors::StageId::new("stage-compile-logic"),
    )
}

fn stage_err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-compile-logic".to_string(),
        message: message.into(),
    })
}

#[cfg(test)]
mod test_fixtures;
#[cfg(test)]
pub(crate) use test_fixtures::synthetic_affine_program;

/// The `stage-compile-logic` pipeline stage.
pub struct CompileLogicStage {
    /// The producer retaining original documents and the shared aggregate selection.
    consumes: Vec<String>,
    /// The catalog receipt graph and its complete native source commitment.
    entities: Vec<(String, Vec<String>)>,
}

impl CompileLogicStage {
    /// Consume the native parse catalog. Source publication and compilation can
    /// run concurrently; the compiler's explicitly selected OPTs and worked
    /// examples remain declared direct inputs.
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-parse-sources".to_string()],
            entities: vec![(
                "stage-parse-sources".to_string(),
                vec![crate::stages::parse_sources::GRAPH_SOURCE_CATALOG.to_string()],
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

/// Keep the throwing boundary coupled to the original owner dispositions and
/// the gates already evaluated by the shared compilation. A rejected owner or
/// leg remains fatal even when it has no claimed law requiring execution.
fn compile_source_projections(
    theory: &Arc<CompiledTheory>,
    program: &LogicProgram,
) -> gmeow_errors::Result<(
    CompiledArtifacts,
    gmeow_logic::correspondence_exec::presentation::source::SourceMerges,
    gmeow_logic::correspondence_exec::axes::SourceAxes,
)> {
    for owner in theory.owner_lowerings() {
        if matches!(
            owner.family,
            OwnerFamily::Correspondence
                | OwnerFamily::CorrespondenceComposition
                | OwnerFamily::TransactionProgram
        ) && owner.disposition == OwnerDisposition::Rejected
        {
            let detail = owner
                .diagnostics
                .iter()
                .map(|&index| theory.diagnostics()[index].message.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(stage_err(format!(
                "rejected authored {:?}: {detail}",
                owner.family
            )));
        }
    }
    let merges = gmeow_logic::correspondence_exec::presentation::source::execute(
        Arc::clone(theory),
        gmeow_logic::correspondence_exec::presentation::PresentationLimits::default(),
    )?;
    let artifacts = compile_program(program, gmeow_logic::correspondence_exec::program_verdicts)
        .map_err(|error| stage_err(format!("compile: {error}")))?;
    if let Some(report) = &artifacts.correspondence_gates {
        assert_gates(report)
            .map_err(|error| stage_err(format!("authored correspondence gate: {error}")))?;
    }
    let axes = gmeow_logic::correspondence_exec::axes::execute(Arc::clone(theory))?;
    Ok((artifacts, merges, axes))
}

/// Form the single shipped correspondence program from the complete compiled
/// source and the explicitly selected worked cell. Duplicate identities are an
/// authoring conflict, never an order-dependent override. Program-level
/// preservation is the lattice join (worst preservation wins).
fn merge_correspondence_programs(
    source: Option<CorrespondenceProgram>,
    selected: CorrespondenceProgram,
) -> gmeow_errors::Result<CorrespondenceProgram> {
    let Some(source) = source else {
        return Ok(selected);
    };
    let preservation =
        gmeow_errors::BoundedLattice::join(source.preservation, selected.preservation);
    let mut correspondences = source.correspondences;
    correspondences.extend(selected.correspondences);
    reject_duplicate_identity(
        "correspondence",
        correspondences.iter().map(|value| value.iri.as_str()),
    )?;
    let mut compositions = source.compositions;
    compositions.extend(selected.compositions);
    reject_duplicate_identity(
        "correspondence composition",
        compositions.iter().map(|value| value.iri.as_str()),
    )?;
    let mut legs = source.leg_programs;
    legs.extend(selected.leg_programs);
    reject_duplicate_identity(
        "correspondence transaction program",
        legs.iter().map(|value| value.iri.as_str()),
    )?;
    Ok(CorrespondenceProgram::new(correspondences, preservation)
        .with_compositions(compositions)
        .with_leg_programs(legs))
}

fn reject_duplicate_identity<'a>(
    kind: &str,
    identities: impl IntoIterator<Item = &'a str>,
) -> gmeow_errors::Result<()> {
    let mut seen = BTreeSet::new();
    for identity in identities {
        if !seen.insert(identity) {
            return Err(stage_err(format!(
                "duplicate {kind} identity <{identity}> while assembling the shipped program"
            )));
        }
    }
    Ok(())
}

impl Stage for CompileLogicStage {
    fn id(&self) -> &str {
        "stage-compile-logic"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    /// The parse-stage receipt and native payload commitment authenticate every
    /// original source input without duplicating the corpus in a transport graph.
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
        // The structural cache stores complete typed Logic, relational and correspondence
        // handles. Native values are required because graph/logic is an
        // intentionally lossy governed projection; serving a reverse-parsed shorter
        // program would violate no-optionality.
        CachePolicy::Persistent
    }
    fn impl_version(&self) -> &str {
        "compile-logic.v40-leg-program-roundtrip"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
        // Explicit OPT, worked-example and counterexample selections are direct inputs.
        // Canonical source and augmentation freshness come from the parse-stage
        // receipt graph and complete typed catalog commitment.
        let mut files = vec![
            root.join(OPT_SOURCE_PATH),
            root.join(OPT_TEST_DATATYPES_PATH),
            root.join(PATH_SHAPES_EXAMPLE_PATH),
            root.join(CORRESPONDENCE_EXAMPLE_PATH),
            root.join(RCHOPS21_PLAN_SOURCE_PATH),
            root.join(RCHOPS21_OBSERVED_SOURCE_PATH),
            root.join(REASONING_PROGRAMS_EXAMPLE_PATH),
        ];
        files.extend(validation_fixtures::input_files(root));
        files.extend(formula_fixtures::input_files(root));
        files.extend(projection_fixtures::input_files(root));
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let catalog = crate::stages::parse_sources::catalog(&input)?;
        let theory = catalog.compiled_logic()?;
        let ontology = theory.source().dataset();
        // The immutable source theory remains coupled to its exact native roots
        // and diagnostics. This projection program additionally carries explicitly
        // selected OPT/example products; it is not the source-execution authority.
        let program = theory.program().clone();
        let mut diagnostics = theory.diagnostics().to_vec();
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
        // Derive validation views from the same retained source selection,
        // before carrier graph placement and public literal projection.
        validation_shapes.extend(
            gmeow_logic_compile::frontend::derive_validation_shapes(ontology)
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
            gmeow_logic_compile::frontend::functional_carrier_integrity(ontology);
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
        // Canonical constraints already belong to the complete shared compilation.
        // Keep them intact; only the explicitly selected OPT products are added.
        validation_shapes.extend(program.validation_shapes.iter().cloned());
        let program = program.with_validation_shapes(validation_shapes);

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
        let mut reasoning_programs = program.reasoning_programs.clone();
        reasoning_programs.extend(reasoning_programs_program.reasoning_programs);
        let program = program.with_reasoning_programs(reasoning_programs);

        // The complete catalog supplies the source correspondences and their legs.
        // Execute and enforce their gates once, retaining the compiled evidence.
        let (mut arts, merges, axes) = compile_source_projections(&theory, &program)?;
        let authored_lift = arts.correspondence_gates.as_ref().map(liftability);

        // Correspondence carrier lane (F4): derive every supported put leg in the authored
        // worked program, run the five gates as a HARD FAIL, and fold the gate-derived
        // liftability statistic into the report header. This explicitly selected program remains
        // separate from the source catalog, so its domain facts do not become source
        // axioms. Its counts join those of the source correspondences below.
        // Read the program from its authored `logic:` TTL (the honest dogfooded
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
        // Discharge executable lens laws engine-adjacent and gate on the resulting
        // per-correspondence verdicts — the gates themselves stay execution-free.
        let gate_verdicts = gmeow_logic::correspondence_exec::program_verdicts(&gated);
        let gate_report = evaluate_gates(&gated, &[], &gate_verdicts);
        assert_gates(&gate_report).map_err(|e| stage_err(format!("correspondence gate: {e}")))?;
        let lift = liftability(&gate_report);
        let selected_correspondence_count = gated.correspondences.len();
        // Ship one complete typed program. The source compiler has already
        // derived and checked its candidate put legs; the selected worked program
        // has just passed the same native law/gate authority above. No source
        // program is discarded behind a worked-example-only handle.
        let correspondence =
            merge_correspondence_programs(arts.correspondence_program.take(), gated)?;
        let physical_plans =
            gmeow_logic::correspondence_exec::physical_plan::PreparedCompositionProgram::prepare(
                &correspondence,
                gmeow_logic::correspondence_exec::physical_plan::CompositionPlanLimits::default(),
            )?;
        let rchops21_source =
            std::fs::read_to_string(input.root.join(RCHOPS21_PLAN_SOURCE_PATH))
                .map_err(|error| stage_err(format!("read {RCHOPS21_PLAN_SOURCE_PATH}: {error}")))?;
        let rchops21_dataset = parse_dataset(rchops21_source.as_bytes(), "text/turtle", None)
            .map_err(|error| stage_err(format!("parse {RCHOPS21_PLAN_SOURCE_PATH}: {error}")))?;
        let rchops21_projection = gmeow_logic::correspondence_exec::plan_projection::project_plan(
            &rchops21_dataset,
            "urn:gmeow:plan:rchops21",
            None,
            gmeow_logic::correspondence_exec::plan_projection::PlanProjectionLimits::default(),
        )
        .map_err(|error| {
            stage_err(format!(
                "project RCHOPS21 process correspondence from {RCHOPS21_PLAN_SOURCE_PATH}: {error}"
            ))
        })?;
        let rchops21_observed_source = std::fs::read_to_string(
            input.root.join(RCHOPS21_OBSERVED_SOURCE_PATH),
        )
        .map_err(|error| stage_err(format!("read {RCHOPS21_OBSERVED_SOURCE_PATH}: {error}")))?;
        let rchops21_observed =
            parse_dataset(rchops21_observed_source.as_bytes(), "text/turtle", None).map_err(
                |error| stage_err(format!("parse {RCHOPS21_OBSERVED_SOURCE_PATH}: {error}")),
            )?;
        let rchops21_recovery =
            gmeow_logic::correspondence_exec::plan_projection::recover_planned_schema_skeleton(
                &rchops21_projection,
                &rchops21_observed,
            )
            .map_err(|error| {
                stage_err(format!(
                    "recover RCHOPS21 planned skeleton from {RCHOPS21_OBSERVED_SOURCE_PATH}: {error}"
                ))
            })?;
        // Count-ownership seam (Seam 1): compile-logic no longer writes the FINAL
        // `correspondence_count` / `lawful_uplift_count` into the report header. It ships the
        // source-and-example BASE explicitly in native report inputs (`base_correspondence_count` /
        // `base_lawful_uplift_count`, populated below from `gated`/`lift`), and mappings'
        // `fold_up_projection_audit` is the SINGLE writer that composes base + external-term
        // audit into the committed counts. Force the header's count fields to 0 so the
        // header carries no correspondence/uplift base; the native inputs retain
        // both the source and selected-example gate totals for that single writer.
        arts.report_header.correspondence_count = 0;
        arts.report_header.lawful_uplift_count = 0;
        arts.report_header.claimed_uplift_count = 0;

        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(
            CORRESPONDENCE_PLANS_PATH.into(),
            physical_plans.report_json()?,
        );
        let mut rchops21_projection_json = serde_json::to_vec_pretty(&rchops21_projection)
            .map_err(|error| stage_err(format!("encode RCHOPS21 plan projection: {error}")))?;
        rchops21_projection_json.push(b'\n');
        artifacts.insert(
            RCHOPS21_PLAN_PROJECTION_PATH.into(),
            rchops21_projection_json,
        );
        let mut rchops21_recovery_json = serde_json::to_vec_pretty(&rchops21_recovery)
            .map_err(|error| stage_err(format!("encode RCHOPS21 plan recovery: {error}")))?;
        rchops21_recovery_json.push(b'\n');
        artifacts.insert(RCHOPS21_PLAN_RECOVERY_PATH.into(), rchops21_recovery_json);
        artifacts.insert(
            "generated/logic/presentation-merges.json".into(),
            merges.report()?,
        );
        artifacts.insert(
            "generated/logic/correspondence-axes.json".into(),
            axes.report()?,
        );
        let grounding = catalog.compiled_document(SOURCE_PATH, Some(SOURCE_IRI.to_owned()))?;
        abduction::record(&grounding, &mut artifacts)?;
        roundtrip::record(grounding.program(), &mut artifacts)?;
        vocabulary_fixtures::record(catalog.document(SOURCE_PATH)?, &mut artifacts)?;
        validation_fixtures::record(catalog, input.root, &mut artifacts)?;
        formula_fixtures::record(input.root, &mut artifacts)?;
        projection_fixtures::record(input.root, &mut artifacts)?;
        // The nine projection serializations, byte-for-byte as the compiler produced
        // them (RDF targets are reconciled by graph isomorphism, text targets by bytes).
        artifacts.insert(
            OWL_DL_PATH.to_string(),
            arts.owl_dl.into_text().content.into_bytes(),
        );
        artifacts.insert(
            OWL_EL_PATH.to_string(),
            arts.owl_el.into_text().content.into_bytes(),
        );
        artifacts.insert(DATALOG_PATH.to_string(), arts.datalog.into_bytes());
        artifacts.insert(N3_PATH.to_string(), arts.n3.into_bytes());
        // gUFO rides as an RDF-fanout named graph: emit EXACTLY the canonical fold
        // (shared prefix authority, no banner) so the superset gate reconstructs it.
        artifacts.insert(
            GUFO_PATH.to_string(),
            purrdf::turtle_normalize::render(
                &arts.gufo.dataset,
                &crate::stages::superset::rdf_prefixes(),
            )
            .into_bytes(),
        );
        // Keep the canonical RDF-1.2 projection: it is BOTH a committed artifact AND
        // the backing graph the typed Logic handle (C6) pins to.
        let canonical_dataset = Arc::clone(&arts.canonical_rdf12.dataset);
        let canonical_rdf12 = arts.canonical_rdf12.into_text().content;
        artifacts.insert(
            CANONICAL_RDF12_PATH.to_string(),
            canonical_rdf12.into_bytes(),
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
        // Share compact native report inputs with mappings. Projection bodies stay
        // in their artifact lanes; the complete loss ledger moves into the immutable
        // publication and is encoded only at the persistent action boundary.
        let report_inputs = LogicReportInputs {
            header: arts.report_header,
            // The source-and-example BASE mappings composes with the external-term
            // up-projection audit to form the committed correspondence/uplift counts.
            base_correspondence_count: selected_correspondence_count
                + authored_lift.map_or(0, |ledger| ledger.total),
            base_lawful_uplift_count: lift.lawful + authored_lift.map_or(0, |ledger| ledger.lawful),
            projections: arts
                .logic_projections
                .into_iter()
                .map(ProjectionReportRow::from)
                .collect(),
            loss: arts.loss,
        };

        // The relational-core lowering (C8): lower the program's Horn rules into the
        // engine-agnostic Datalog±-with-stratified-negation dialect, then project it into
        // one native RDF graph. Keep the projection: it backs BOTH a terminal
        // artifact and the graph the typed RelationalCore handle pins to. The
        // lowering runs EXACTLY ONCE here; every downstream consumer reads the typed handle
        // (or the folded graph), never re-lowering.
        let relational_core = lower_program_with_formulas(&program);
        let relational_core_projection = project_relational_core_dataset(&relational_core)?;
        artifacts.insert(
            RELATIONAL_CORE_PATH.to_string(),
            super::superset::canonical_ntriples(&relational_core_projection)?,
        );

        // The correspondence carrier lane (C10): the complete typed source program plus
        // the executable worked program. Constructed ONCE here, projected ONCE here, then carried BOTH
        // as the typed `PipelineHandle::Correspondence` payload AND its backing
        // `graph/correspondence` projection. The overclaim gate keeps every relation at its
        // certified strength, and the physical-plan report binds every admitted composition
        // to this same complete program.
        let correspondence_projection = project_correspondence_dataset(&correspondence)?;
        correspondence_roundtrip::record(
            &correspondence,
            &correspondence_projection,
            &mut artifacts,
        )?;
        artifacts.insert(
            CORRESPONDENCE_PATH.to_string(),
            crate::stages::superset::canonical_ntriples(&correspondence_projection)?,
        );

        // The compile diagnostics: the front-end parse findings (already coded
        // `logic-compile.<code>` by the shared bridge) UNIONED with the loss ledger's
        // OWN witness projection. Rather than hand-build identity-less notes, project the
        // single runtime loss store (`report_inputs.loss`) through `to_finding`: each structural
        // and actual lossy-drop witness surfaces as a finding carrying its stable
        // `finding_iri` / `anchor_iri` and — for an actual drop — the wired antecedent DAG
        // edge (its causing structural-limitation witness) as a structured antecedent +
        // related location. That closing DAG is exactly what the diagnostic meta-fold below
        // joins on to derive `gmeow:findingRootCause` on the SHIPPED bundle (the hand-built
        // notes carried no such identity, so the meta chase derived nothing).
        let mut report = gmeow_logic::logic_diagnostics::diagnostics_report(&diagnostics);
        for finding in report_inputs.loss.project_report(TOOL).findings {
            report.add_finding(finding);
        }
        // Whole-catalog findings cannot all be assigned to the grounding module.
        // Preserve actual locations and logical anchors; the shared source theory
        // retains original document bindings for precise attribution.
        // Normalize for a deterministic committed artifact (mirrors the PyO3 surface).
        report.normalize();
        // The diagnostic meta-fold: the authored `gmeow:DiagnosticMetaRule` rules (from
        // slices/grounding/logic/module.ttl) + the `gmeow:categoryPolarity` wiring (from
        // slices/core/diagnostics/module.ttl) discovered BY TYPE off the merged authored
        // catalog's compiled theory and native source. The loss
        // findings above now carry closing antecedent DAGs, so this fold derives the
        // root-cause / cluster / cross-node-glut meta-findings on the SHIPPED bundle.
        let meta = crate::stages::meta_findings::MetaProgram::from_compiled_theory(&theory)
            .map_err(|e| stage_err(format!("diagnostic meta-fold: {e}")))?;
        // The run ledger keeps its existing pre-meta findings; native docs share
        // the renderer's final enriched report independently of that projection.
        let nodes = crate::stages::diag_render::finding_nodes(&report, self.id());
        let mut rendered = render_diagnostics_artifacts(
            self.id(),
            report,
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
        )?;
        rendered.artifacts.append(&mut artifacts);

        // Share the program and mandatory report publication, pinned to the
        // canonical RDF-1.2 projection of THIS program
        // folded into the `graph/logic` named graph. A downstream consumer takes the
        // typed `Arc<LogicProgram>` and never re-parses the logic graph. A persistent
        // hit restores the authenticated typed program; the graph is a governed
        // projection and cannot reconstruct every source capability.
        let bundle = build_logic_bundle(
            CompiledLogicPublication {
                program: Arc::new(program),
                report: report_inputs,
            },
            canonical_dataset,
            relational_core,
            relational_core_projection,
            correspondence,
            correspondence_projection,
            rendered,
        )?;
        // FORWARD diagnostics fold: the compile report's findings are the SINGLE source
        // of both the shipped `graph/diagnostics` RDF (folded into the bundle above) AND
        // the run-level DiagLedger. Project them once to pre-lowered DiagNodes, carry
        // them on the product's `diagnostics:nodes` blob (so a cache hit re-serves them),
        // and hand them up as `StageOutput.diags` for the scheduler to fold on a fresh run.
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
/// projection, with the typed [`PipelineHandle::CompiledLogic`] handle pinned to that graph's
/// canonical digest.
///
/// The handle carries the live program and compact, complete report inputs;
/// its backing graph is the SAME projection `stage-snapshot` folds into `gmeow.gts`, so
/// the in-graph carriage and the handle are pinned to one identity. `pin_handle`
/// HARD-fails on a digest mismatch, so a handle that disagrees with its backing graph
/// can never be attached (no-optionality, fail-closed).
fn build_logic_bundle(
    publication: CompiledLogicPublication,
    canonical_rdf12: Arc<RdfDataset>,
    relational_core: RelationalCoreProgram,
    relational_core_projection: Arc<RdfDataset>,
    correspondence: CorrespondenceProgram,
    correspondence_projection: Arc<RdfDataset>,
    rendered: RenderedDiagnostics,
) -> Result<PipelineBundle<PipelineHandle>, gmeow_errors::Diag> {
    // The logic-compile diagnostics RDF also rides the carrier, in the shared
    // `graph/diagnostics` named graph, so the presenter unions it with the SHACL
    // diagnostics as a pure keyed fold (PIPELINE_SPINE §4) instead of re-parsing the byte
    // artifact. It is object-level-inert (a Finding graph), so it never reaches the reason
    // EDB (which projects only logic / relational-core). The byte lane is
    // kept for terminal output readers; docs borrow the complete native Report.
    let RenderedDiagnostics {
        artifacts,
        dataset: diag_dataset,
        report,
    } = rendered;
    // Retain the original projections, route their complete RDF surfaces, and
    // materialize the final bundle once. Each independent input has its own blank
    // scope; diagnostics retain their graph placement. No rooted intermediates
    // or owned-term union precede this publication.
    let dataset = native_carrier::assemble(
        [
            canonical_rdf12,
            relational_core_projection,
            correspondence_projection,
        ],
        diag_dataset,
    )?;
    let mut bundle = bundle_from_artifacts_over(dataset, artifacts, DatasetProvenance::new());
    crate::bundle::pin_diagnostics(
        &mut bundle,
        "stage-compile-logic",
        Arc::new(crate::bundle::DiagnosticsPublication::producer(
            crate::bundle::DiagnosticReportOwner::CompileLogic,
            report,
        )?),
    )?;
    let pinned = bundle.graph_digest(GRAPH_LOGIC);
    bundle
        .pin_handle(
            GRAPH_LOGIC,
            PipelineHandle::CompiledLogic(Arc::new(publication)),
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

#[path = "compile_logic.tests.rs"]
#[cfg(test)]
mod tests;
