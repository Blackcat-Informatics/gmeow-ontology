// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-case orchestration (`run_case`).
//!
//! Drives the `gmeow_logic` native cores for one conformance case and assembles a
//! typed [`CaseOutputs`] by calling the SAME native functions the PyO3 surface wraps
//! (compile → certify → materialize+explain / foundation → answers). There is no
//! PyO3, no Python, and no second engine in this path — the harness is a second
//! *caller* of the engine cores, so its artifacts are identical by construction
//! (the retired Python `logic_runner.run` this replaced has since been removed).
//!
//! Witnesses (`witnesses.json`) are intentionally NOT produced: the diff phase
//! never compared them — they are a bless-only side file — so omitting them
//! changes no gate verdict.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use gmeow_errors::Diag;
use gmeow_logic::explain::{Row, explain_all};
use gmeow_logic::foundation::{AntiRigidityPolicy, evaluate as foundation_evaluate};
use gmeow_logic::materialize::{MaterializationLimits, materialize_program};
use gmeow_logic::query_ir::{Budget, parse_query_program};
use gmeow_logic::result::PreservationClaim;
use gmeow_logic::seam::{BudgetStatus, WorldFactSnapshot};
use gmeow_logic::store::WorldStore;
use gmeow_logic::teleology::materialize_teleology as teleology_evaluate;
use gmeow_logic_compile::frontend::{Diagnostic, Severity, parse_logic_dataset};
use gmeow_logic_compile::projections::compile_program;
use serde::{Deserialize, Serialize};

use crate::error::RunFailed;
use crate::native_observation::{NativeCaseObservation, NativeCaseOperation};
use crate::profile::{BudgetParams, Profile, VerdictMode};
use crate::serialize::VerdictStatus;
use crate::{profile, serialize};

/// A single materialized quad in the runner's flat string form (mirrors the
/// Python `DerivedQuad` surface the downstream consumers read). The subject is a
/// BARE IRI; the object is in N3 form (`<iri>` or a literal).
#[derive(Debug, Clone)]
pub struct RunnerQuad {
    /// Contextual modal evidence survives the foundation-to-explanation bridge.
    pub modal_evaluation: Option<gmeow_logic::modal::ModalEvaluation>,
    pub graph: String,
    pub subject: String,
    pub predicate: String,
    pub obj: String,
    pub derivation_id: String,
    pub rule_iri: String,
    pub source_quad_ids: Vec<String>,
    /// The native governor's PER-QUAD budget verdict (`ok` / `exhausted` / `partial`).
    /// A quad whose predicate's stratum settled is `ok` (its extension is final) even when
    /// the RUN exhausted; a quad from the cut / unreached strata is `exhausted`. Surfaced
    /// into the `quad-status.json` golden (the only artifact that carries the per-quad
    /// stamp — `materialized.nq` compares by graph isomorphism, with no status column).
    pub budget_status: String,
}

/// One explanation skeleton, keyed by its target quad reifier (the match key the
/// `expected/explanation/*.md` goldens use).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplanationOut {
    pub target_quad_reifier: String,
    pub cited_iris: BTreeSet<String>,
    /// Full Markdown rendering of this explanation, suitable for writing to
    /// `expected/explanation/{hash}.md`.
    pub markdown: String,
}

/// One path-shape projection entry in the conformance output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathProjectionOut {
    /// IRI of the projected `logic:PathShape`.
    pub shape_iri: String,
    /// The serialized extended SPARQL property path.
    pub property_path: String,
    /// The depth-bounded Datalog rule scheme (native-engine syntax).
    pub datalog: String,
}

/// The projection artifacts for one case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionOutputs {
    /// The four RDF targets (`owl-dl`, `owl-el`, `gufo`, `canonical-rdf12`) as
    /// Turtle, compared by graph-isomorphism.
    pub rdf: BTreeMap<String, String>,
    /// The preservation report as Turtle (graph-isomorphism comparison).
    pub report_turtle: String,
    /// The preservation ledger JSON (`{target: {preservation, complexity, lossy_drops}}`).
    pub ledger: serde_json::Value,
    /// Plain-text projections (`datalog`, `n3`) — kept for bless; not diffed.
    pub text: BTreeMap<String, String>,
    /// Per-shape property-path projections (`logic:PathShape` → SPARQL + Datalog).
    /// Empty when the program declares no path shapes — never absent.
    pub path_projections: Vec<PathProjectionOut>,
    /// The closed-world SHACL Core projection of the program's `logic:ValidationShape`s
    /// (Turtle, graph-isomorphism comparison against `expected/projections/shacl-core.ttl`).
    /// Empty string when the program declares no validation shapes — never absent.
    pub shacl_core: String,
    /// The closed-world ShEx projection of the program's validation shapes (ShExC, exact-text
    /// comparison against `expected/projections/shapes.shex`). Empty string when shape-free.
    pub shex: String,
    /// The per-target validation-shape residue set (`{shacl-core: [...], shex: [...]}`,
    /// deterministically sorted + deduped) — the constructs each shape surface cannot faithfully
    /// hold, carried in the canonical logic: layer. Gates `expected/projections/residue.json`.
    pub residue: serde_json::Value,
}

/// Everything one case run produces, ready for `diff_case` / bless.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseOutputs {
    pub case_id: String,
    pub materialized_nquads: String,
    /// The per-quad budget-status golden (`[{quad, status}]`, deterministically sorted).
    /// Surfaces the frontier-aware PER-QUAD stamp that `materialized_nquads` (compared by
    /// graph isomorphism, no status column) cannot carry. Empty array when no quads.
    pub materialized_quad_status: serde_json::Value,
    pub projections: ProjectionOutputs,
    pub explanations: Vec<ExplanationOut>,
    pub verdicts: serde_json::Value,
    pub certification: serde_json::Value,
    pub budget_status: String,
    pub incomplete: bool,
    /// The native forward governor's completion frontier for this case's world
    /// materialization: which strata / predicates are settled and how many derivations
    /// were committed. Surfaced into `budget.json` (as `strata_completed` /
    /// `strata_total` / `saturated`) ONLY when the case declares a step/derivation budget
    /// (`max_steps` / `max_rule_firings`) — an ungoverned run reports the trivially
    /// complete frontier, which adds no diagnostic signal.
    pub frontier: gmeow_logic::query_ir::CompletionFrontier,
    /// `{query_stem: {"bindings": [...], "status": "...", "preservation": {...}}}` for
    /// each `queries/*.logic`.
    pub answers: BTreeMap<String, serde_json::Value>,
    /// The materialization's runtime preservation judgment (downstream disclosure):
    /// `{polarities, unsupported_constructs}`. `{exact}` for the faithful chase /
    /// foundation paths; `{sound-under}` naming the dropped rules for the
    /// non-stratifiable EDB-echo path. Distinct from the compile-time projection
    /// ledger in `projections.ledger`.
    pub preservation: serde_json::Value,
    /// The five-gate correspondence report (F4), evaluated with the case's declared
    /// compositions. `Some` iff the program authors `logic:Correspondence` individuals;
    /// `None` (no golden gated) for every correspondence-free case.
    pub correspondence_gates: Option<serde_json::Value>,
    /// The Common Logic round-trip verdict: `{ "<dialect>": {"round_trip":"pass"},
    /// "cross_dialect": "pass" }`. `Some` only for a `cl-roundtrip` case (gating the
    /// `expected/cl-dialects.json` golden); `None` for every other verdict mode.
    pub cl_dialects: Option<serde_json::Value>,
}

/// The four RDF projection targets compared by graph-isomorphism.
const RDF_TARGETS: [&str; 4] = ["owl-dl", "owl-el", "gufo", "canonical-rdf12"];

/// The sentinel rule IRI marking an asserted (EDB) input fact.
const ASSERT_RULE_IRI: &str = "https://blackcatinformatics.ca/logic/assert";

/// The profile IRI foundation cases materialize under (matches the native
/// evaluator's stamped profile and the committed goldens).
const POSITIVE_HORN_PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";

/// Build a run-failure diagnostic from a preserved message.
fn run_fail(detail: String) -> Diag {
    Diag::of_kind(RunFailed { detail })
}

/// Execute one case with the selected compiled library and consistency provider.
/// The producer supplies a shared native action; synthetic tests may supply a
/// direct evaluator over their own constructed inputs.
///
/// # Errors
/// Refuses malformed inputs, missing capabilities and engine failures. Execution
/// errors remain errors and cannot stand in for a semantic capability gap.
pub fn run_case(
    case_dir: &Path,
    library: &RuleLibrary<'_>,
    native: &impl Fn(&Path, NativeCaseOperation) -> gmeow_errors::Result<NativeCaseObservation>,
) -> gmeow_errors::Result<CaseOutputs> {
    let case_id = crate::paths::case_id(case_dir);
    let prefix = |msg: String| run_fail(format!("case {case_id}: {msg}"));

    // ── Profile ──────────────────────────────────────────────────────────────
    let profile_text = std::fs::read_to_string(case_dir.join("profile.json"))
        .map_err(|e| prefix(format!("cannot read profile.json: {e}")))?;
    let profile_value: serde_json::Value = serde_json::from_str(&profile_text)
        .map_err(|e| prefix(format!("cannot parse profile.json: {e}")))?;
    let profile = profile::parse_profile(&case_id, &profile_value)?;

    // ── Consistency mode ──────────────────────────────────────────────────────
    // External entailment/SZS cases reason over their RDF EDB through the native
    // DL consistency path, NOT the logic-compile/materialize chase. Branch BEFORE
    // reading/compiling `input.logic.ttl` (which a consistency case does not use).
    if profile.verdict_mode == VerdictMode::Consistency {
        return run_consistency_case(&case_id, case_dir, &|path| match native(
            path,
            NativeCaseOperation::Consistency,
        )? {
            NativeCaseObservation::Consistency(observation) => Ok(observation),
            other => Err(prefix(format!(
                "consistency provider returned {:?}",
                other.operation()
            ))),
        });
    }

    if profile.verdict_mode == VerdictMode::ClassSourceAdmission {
        let path = case_dir.join("input.nq");
        if !path.is_file() {
            return Err(prefix(
                "class-source-admission requires input.nq".to_owned(),
            ));
        }
        let NativeCaseObservation::ClassSourceAdmission(observation) =
            native(&path, NativeCaseOperation::ClassSourceAdmission)?
        else {
            return Err(prefix(
                "source-admission provider returned a different operation".to_owned(),
            ));
        };
        let input_bytes = std::fs::read(&path).map_err(|error| prefix(error.to_string()))?;
        if observation.input_blake3 != *blake3::hash(&input_bytes).as_bytes() {
            return Err(prefix(
                "source-admission provider returned another input identity".to_owned(),
            ));
        }
        observation.admission.validate()?;
        if let Some(expected) = profile_value
            .get("source_admission_status")
            .and_then(serde_json::Value::as_str)
        {
            if expected != observation.admission.observation_status() {
                return Err(prefix(
                    "source-admission observation does not satisfy the selected expected status"
                        .to_owned(),
                ));
            }
        }
        let mut out = empty_outputs(case_id.clone());
        out.verdicts = serde_json::to_value(observation.admission)
            .map_err(|error| prefix(error.to_string()))?;
        return Ok(out);
    }

    // ── Common Logic round-trip mode ──────────────────────────────────────────
    // A `cl-roundtrip` case gates the CLIF/CGIF/XCL Exact projections (IR round-trip
    // isomorphism + cross-dialect equivalence) and pins their canonical rendering. It
    // does NOT materialize — branch before the compile/certify/chase, like consistency.
    if profile.verdict_mode == VerdictMode::CommonLogic {
        return run_cl_roundtrip_case(&case_id, case_dir, &profile.shipped_rules, library);
    }

    // ── Compile (frontend → canonical IR → projections + ledger) ─────────────
    // The unsupported-contract firewall lives in `compile_case_program`,
    // shared with the CL round-trip path so neither can evaluate an unsound program.
    let (program, dataset) = match compile_case_program(
        &case_id,
        case_dir,
        profile.expect_unsupported,
        &profile.shipped_rules,
        library,
    )? {
        // An `expect_unsupported` case: the program must not proceed — return empty
        // outputs so the diff phase sees no goldens to compare (no `expected/` tree).
        CompileOutcome::Unsupported => return Ok(empty_outputs(case_id)),
        CompileOutcome::Program(program, dataset) => (*program, dataset),
    };

    // ── Validation shapes (closed-world SHACL Core / ShEx projection) ─────────
    // Derive the closed-world validation shapes from the case input's OWL restriction axioms —
    // the SAME `derive_validation_shapes` the pipeline's compile_logic stage runs over the merged
    // authored ontology. A case authoring no GMEOW-namespace OWL restriction derives no shape, so
    // the historical corpus stays byte-identical; the `validation/` category authors restrictions
    // that project to shacl-core / shex documents + a preservation-ledger residue.
    let program = attach_validation_shapes(&case_id, &dataset, program)
        .map_err(|e| prefix(format!("attach validation shapes: {e}")))?;

    // ── Universal CL round-trip invariant ─────────────────────────────────────
    // Every materialized case's IR must round-trip through all three ISO 24707 dialects
    // (CLIF/CGIF/XCL) with IR isomorphism AND be cross-dialect equivalent. This dogfoods
    // the Exact projection claim over the whole corpus; a failure is a dialect bug, never
    // a case bug — hard-fail (the overclaim gate forbids an Exact target dropping data).
    gmeow_logic_compile::cl_roundtrip::assert_all_dialects_isomorphic(&program)
        .map_err(|e| prefix(format!("CL dialect round-trip invariant failed: {e}")))?;

    // Discharge every authored correspondence's lens law by EXECUTION and thread the
    // per-correspondence verdicts into BOTH `compile_program` (its internal gates) and the
    // re-evaluation below with the case's compositions — the gates themselves are
    // execution-free, so the harness (like the pipeline) supplies the engine verdicts. A
    // correspondence-free case yields an empty map (the gates never run). A deliberately-RED
    // fixture (an authored put that is not the derived inverse) discharges as
    // ObligationViolated, matching its blessed-RED gate report.
    let arts = compile_program(&program, gmeow_logic::correspondence_exec::program_verdicts)
        .map_err(|e| prefix(format!("compile failed: {e}")))?;
    // ── Static certification against the declared profile ────────────────────
    // Thread the program's contract `logic:EvolutionMode` so a facet-direct case
    // (e.g. transaction-path) carries the right evolution_class; no facet selected
    // collapses to StaticEvolution at the certify boundary.
    let verdict = gmeow_logic::certify::certify_program(&program, &profile.semantic_profile)
        .map_err(|e| prefix(format!("certify failed: {e}")))?;
    let certification = serialize::certification_to_json(&verdict);

    // ── Materialization (+ explanations) ─────────────────────────────────────
    let input_nq = read_optional(case_dir, "input.nq")?;
    let (quads, budget_status, incomplete, mat_preservation, mat_frontier) =
        if profile.foundation_lowering {
            materialize_foundation(&case_id, &input_nq, &profile)?
        } else if profile.teleology_lowering {
            materialize_teleology(&case_id, &input_nq, &profile)?
        } else {
            materialize_default(&case_id, &program, &input_nq, &profile)?
        };
    let explanations = run_explanations(&case_id, &quads)?;

    // ── N-Quads serialization + downstream artifacts ─────────────────────────
    let materialized_nquads = serialize::materialized_to_nquads(&quads);
    // The per-quad budget stamp (frontier-aware): the ONLY artifact that carries which
    // quads are conclusive (`ok`) versus cut (`exhausted`) under an exhausted run.
    let materialized_quad_status = serialize::quad_status_to_json(&quads);
    // Materialization-mode status: every materializing world is `consistent`,
    // EXCEPT when the budget governor exhausted the chase — then the run is
    // `incomplete` (the external `Unknown`/budget-tripped branch). A clean
    // (non-exhausted) run reproduces the `consistent` golden byte-for-byte.
    let mat_status = if incomplete {
        VerdictStatus::Incomplete
    } else {
        VerdictStatus::Consistent
    };
    let world_counts = serialize::count_worlds(&quads);
    let verdicts = serialize::build_verdicts(&world_counts, |_| mat_status);

    // ── Backward goals ────────────────────────────────────────────────────────
    let answers = resolve_answers(
        &case_id,
        case_dir,
        &materialized_nquads,
        profile.query_profile(),
        &profile.budget_params,
    )?;

    // ── Projections repackaged for the diff ──────────────────────────────────
    let mut rdf = BTreeMap::new();
    rdf.insert(
        "owl-dl".to_string(),
        arts.owl_dl.clone().into_text().content,
    );
    rdf.insert(
        "owl-el".to_string(),
        arts.owl_el.clone().into_text().content,
    );
    rdf.insert("gufo".to_string(), arts.gufo.clone().into_text().content);
    rdf.insert(
        "canonical-rdf12".to_string(),
        arts.canonical_rdf12.clone().into_text().content,
    );
    debug_assert!(RDF_TARGETS.iter().all(|t| rdf.contains_key(*t)));
    let mut text = BTreeMap::new();
    text.insert("datalog".to_string(), arts.datalog.clone());
    text.insert("n3".to_string(), arts.n3.clone());

    let path_projections_out: Vec<PathProjectionOut> = arts
        .path_projections
        .iter()
        .map(|pp| PathProjectionOut {
            shape_iri: pp.shape_iri.clone(),
            property_path: pp.property_path.clone(),
            datalog: pp.datalog.clone(),
        })
        .collect();

    // Validation-shape surfaces: the SHACL Core / ShEx documents compile_program projected from
    // `program.validation_shapes` (pulled from the SAME artifact set, never recomputed), plus the
    // per-target residue set built by reusing the shape-projection residue functions.
    let shacl_core = projection_content(&arts, "shacl-core");
    let shex = projection_content(&arts, "shex");
    let residue = shape_residue_json(&program);

    let projections = ProjectionOutputs {
        rdf,
        report_turtle: arts.report.clone(),
        ledger: serialize::ledger_to_json(&arts.preservation_ledger),
        text,
        path_projections: path_projections_out,
        shacl_core,
        shex,
        residue,
    };

    // ── Correspondence gates (F4) ─────────────────────────────────────────────
    // Re-evaluate the five gates over the DERIVED correspondence program (every put-less
    // cell's put minted by compile_program) with the case's declared compositions, and
    // serialize the report as the `correspondence-gates.json` golden. `None` when the
    // program authors no correspondences (so a correspondence-free case gates nothing new).
    let correspondence_gates = match &arts.correspondence_program {
        Some(derived) => {
            let report = gmeow_logic_compile::projections::correspondence_gates::evaluate_gates(
                derived,
                &profile.compositions,
                &arts.correspondence_verdicts,
            );
            Some(
                serde_json::to_value(&report)
                    .map_err(|e| prefix(format!("serialize correspondence gates: {e}")))?,
            )
        }
        None => None,
    };

    Ok(CaseOutputs {
        case_id,
        materialized_nquads,
        materialized_quad_status,
        projections,
        explanations,
        verdicts,
        certification,
        budget_status,
        incomplete,
        frontier: mat_frontier,
        answers,
        preservation: serialize::preservation_to_json(&mat_preservation),
        correspondence_gates,
        // A materialization case does not gate CL dialect round-trip goldens (the
        // universal invariant above already proved round-trip; only a `cl-roundtrip`
        // case pins the dialect texts + verdict).
        cl_dialects: None,
    })
}

/// Parse the case input as an RDF dataset and derive its closed-world validation shapes from the
/// OWL restriction axioms, attaching them to `program`. Mirrors the pipeline's compile_logic stage
/// (`derive_validation_shapes` over the authored ontology): a case authoring no GMEOW-namespace OWL
/// restriction derives no shape and the program is unchanged. Hard-fails (never silently drops) on a
/// malformed restriction — the same fail-closed contract the derive itself enforces.
fn attach_validation_shapes(
    case_id: &str,
    dataset: &purrdf::RdfDataset,
    program: gmeow_logic_compile::ir::LogicProgram,
) -> gmeow_errors::Result<gmeow_logic_compile::ir::LogicProgram> {
    let prefix = |msg: String| run_fail(format!("case {case_id}: {msg}"));
    let shapes = gmeow_logic_compile::frontend::derive_validation_shapes(dataset)
        .map_err(|e| prefix(format!("derive validation shapes: {e}")))?;
    if shapes.is_empty() {
        return Ok(program);
    }
    Ok(program.with_validation_shapes(shapes))
}

/// The serialized content of one whole-program projection `target` (`"shacl-core"` / `"shex"`) from
/// the compiled artifacts — the SAME document compile_program interned, never recomputed. Empty
/// string when the target is absent (a shape-free program still carries the row with empty content).
fn projection_content(
    arts: &gmeow_logic_compile::projections::CompiledArtifacts,
    target: &str,
) -> String {
    arts.logic_projections
        .iter()
        .find(|p| p.target == target)
        .map(|p| p.content.clone())
        .unwrap_or_default()
}

/// The empty per-target residue value (`{shacl-core: [], shex: []}`) — the residue shape a
/// non-materializing / shape-free case carries so the golden's structure is stable.
fn empty_shape_residue() -> serde_json::Value {
    serde_json::json!({ "shacl-core": [], "shex": [] })
}

/// The per-target validation-shape residue set, reusing the shape-projection residue functions
/// (`shapes::shacl_residue` / `shapes::shex_residue`) over every declared shape. Each target's
/// residue is sorted + deduped so the golden is deterministic regardless of shape order.
fn shape_residue_json(program: &gmeow_logic_compile::ir::LogicProgram) -> serde_json::Value {
    use gmeow_logic_compile::projections::shapes;
    let dedup_sorted = |mut v: Vec<String>| -> Vec<String> {
        v.sort();
        v.dedup();
        v
    };
    let shacl: Vec<String> = dedup_sorted(
        program
            .validation_shapes
            .iter()
            .flat_map(shapes::shacl_residue)
            .collect(),
    );
    let shex: Vec<String> = dedup_sorted(
        program
            .validation_shapes
            .iter()
            .flat_map(shapes::shex_residue)
            .collect(),
    );
    serde_json::json!({ "shacl-core": shacl, "shex": shex })
}

/// Whether `diags` carries at least one `Severity::Error` diagnostic, returning
/// the first one. (`parse_logic_str` keeps semantic errors INSIDE the vec rather
/// than returning `Err`, so the harness inspects them explicitly.)
fn first_error(diags: &[Diagnostic]) -> Option<&Diagnostic> {
    diags.iter().find(|d| d.severity == Severity::Error)
}

/// Whether the compile flagged an `UNSUPPORTED_CONTRACT` `Severity::Error` — the
/// forbidden-facet-combination firewall verdict an `expect_unsupported` case asserts.
fn has_unsupported_contract_error(diags: &[Diagnostic]) -> bool {
    diags
        .iter()
        .any(|d| d.severity == Severity::Error && d.code == "UNSUPPORTED_CONTRACT")
}

/// The outcome of compiling a case's `input.logic.ttl` through the unsupported-contract
/// firewall.
enum CompileOutcome {
    /// The case declared `expect_unsupported` and the compile confirmed it
    /// (`UNSUPPORTED_CONTRACT` `Severity::Error`). The caller must short-circuit WITHOUT
    /// evaluating — the program is unsound by design and must not proceed.
    Unsupported,
    /// A clean program plus the original native dataset used for shape derivation. The `LogicProgram` is boxed to
    /// keep the enum small (it dwarfs the unit `Unsupported` variant otherwise).
    Program(
        Box<gmeow_logic_compile::ir::LogicProgram>,
        std::sync::Arc<purrdf::RdfDataset>,
    ),
}

/// Parse a case's `input.logic.ttl` and apply the unsupported-contract firewall,
/// shared by the materialization path and the CL round-trip path so neither can
/// evaluate an unsound program.
///
/// `parse_logic_str` returns `Err` only on a hard Turtle PARSE failure; a semantic
/// `Severity::Error` (e.g. an `UNSUPPORTED_CONTRACT` forbidden facet combination) rides
/// INSIDE the diagnostics vec with `Ok((..))`, so the firewall inspects them explicitly:
/// an `expect_unsupported` case REQUIRES the flag (else the engine wrongly accepted the
/// contract), and any other `Severity::Error` on a non-`expect_unsupported` case is a
/// hard failure (never evaluate as if the contract were sound).
/// `shipped_rules` names the `logic:Rule` IRIs the case loads from the shipped `logic:`
/// module, resolved by the explicitly supplied [`RuleLibrary`] and merged into the compiled program.
fn compile_case_program(
    case_id: &str,
    case_dir: &Path,
    expect_unsupported: bool,
    shipped_rules: &[String],
    library: &RuleLibrary<'_>,
) -> gmeow_errors::Result<CompileOutcome> {
    let prefix = |msg: String| run_fail(format!("case {case_id}: {msg}"));
    let source = std::fs::read_to_string(case_dir.join("input.logic.ttl"))
        .map_err(|e| prefix(format!("cannot read input.logic.ttl: {e}")))?;
    let dataset = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None)
        .map_err(|e| prefix(format!("input.logic.ttl RDF parse failed: {e}")))?;
    let (program, diagnostics) = parse_logic_dataset(&dataset, None)
        .map_err(|e| prefix(format!("compile parse failed: {}", e.0)))?;

    if expect_unsupported {
        if !has_unsupported_contract_error(&diagnostics) {
            return Err(prefix(format!(
                "profile.json declares \"expect_unsupported\": true but the compile produced \
                 no UNSUPPORTED_CONTRACT Severity::Error — the engine accepted the contract. \
                 Diagnostics: {diagnostics:?}"
            )));
        }
        return Ok(CompileOutcome::Unsupported);
    }

    if let Some(first) = first_error(&diagnostics) {
        return Err(prefix(format!(
            "compile emitted a Severity::Error diagnostic but the case does not declare \
             \"expect_unsupported\": true — refusing to evaluate an unsound program. \
             First error [{}]: {}",
            first.code, first.message
        )));
    }

    let program = merge_shipped_rules(case_id, program, shipped_rules, library)?;
    Ok(CompileOutcome::Program(Box::new(program), dataset))
}

/// Borrowed index of the explicitly selected rule library. The producer owns its
/// compiled source; every case shares this index without reading another checkout
/// or compiling the grounding module inside a test process.
#[derive(Default)]
pub struct RuleLibrary<'source> {
    rules: BTreeMap<&'source str, &'source gmeow_logic_compile::ir::LogicRule>,
}

impl<'source> RuleLibrary<'source> {
    /// Index named rules from a previously compiled source. Unnamed rules cannot
    /// be selected by a profile. Conflicting named declarations fail closed.
    pub fn new(
        program: &'source gmeow_logic_compile::ir::LogicProgram,
    ) -> gmeow_errors::Result<Self> {
        let mut rules = BTreeMap::new();
        for rule in &program.rules {
            if let Some(iri) = rule.scope.provenance.as_deref()
                && rules.insert(iri, rule).is_some()
            {
                return Err(run_fail(format!("rule library repeats named rule {iri}")));
            }
        }
        Ok(Self { rules })
    }
}

/// Merge the profile-declared shipped rules into `program`.
///
/// Every IRI is resolved against the shipped module and HARD-FAILS when absent: that
/// failure IS the pin. A case that re-typed the rule in its own `input.logic.ttl` would
/// stay green after the shipped rule was deleted, so the corpus would pin its own copy
/// rather than what ships; resolving through the module makes deletion or renaming red.
/// Redeclaring a shipped rule locally is likewise a hard failure — two sources of truth
/// for one rule is the condition the resolution exists to remove.
fn merge_shipped_rules(
    case_id: &str,
    program: gmeow_logic_compile::ir::LogicProgram,
    shipped_rules: &[String],
    library: &RuleLibrary<'_>,
) -> gmeow_errors::Result<gmeow_logic_compile::ir::LogicProgram> {
    if shipped_rules.is_empty() {
        return Ok(program);
    }
    let prefix = |msg: String| run_fail(format!("case {case_id}: {msg}"));
    let index = &library.rules;

    let mut program = program;
    for iri in shipped_rules {
        let rule = index.get(iri.as_str()).ok_or_else(|| {
            prefix(format!(
                "profile.json shipped_rules names {iri}, which is not a logic:Rule in the \
                 shipped module (the selected library declares {} named rules)",
                index.len()
            ))
        })?;
        if program
            .rules
            .iter()
            .any(|r| r.scope.provenance.as_deref() == Some(iri.as_str()))
        {
            return Err(prefix(format!(
                "input.logic.ttl redeclares the shipped rule {iri} that profile.json \
                 already loads — author it in exactly one place, the shipped module"
            )));
        }
        program.rules.push((*rule).clone());
    }
    // Restore the canonical rule order `LogicProgram::new` maintains, so the compiled
    // artifacts (and every golden projected from them) do not depend on the order the
    // profile happened to list the rules in.
    program
        .rules
        .sort_by_cached_key(gmeow_logic_compile::ir::LogicRule::sort_key);
    Ok(program)
}

/// Run one `verdict_mode = cl-roundtrip` case.
///
/// Gates the three ISO 24707 dialects (CLIF/CGIF/XCL) as `PreservationKind::Exact`
/// bidirectional projections: the case's IR must round-trip through each dialect with IR
/// isomorphism AND the three reconstructions must be cross-dialect equivalent (proved by
/// [`gmeow_logic_compile::cl_roundtrip::assert_all_dialects_isomorphic`]). It then pins
/// the canonical-fixpoint dialect renderings as byte-exact goldens
/// (`expected/projections/gmeow.{clif,cgif,xcl}`) and emits the `cl-dialects.json`
/// verdict. A CL round-trip case does NOT materialize (like consistency mode); it carries
/// only its dialect-text goldens + the verdict.
fn run_cl_roundtrip_case(
    case_id: &str,
    case_dir: &Path,
    shipped_rules: &[String],
    library: &RuleLibrary<'_>,
) -> gmeow_errors::Result<CaseOutputs> {
    let prefix = |msg: String| run_fail(format!("case {case_id}: {msg}"));

    // A cl-roundtrip case never declares `expect_unsupported` (an unsound program cannot
    // round-trip); `compile_case_program(.., false)` therefore never yields `Unsupported`.
    let program = match compile_case_program(case_id, case_dir, false, shipped_rules, library)? {
        CompileOutcome::Program(program, _dataset) => *program,
        CompileOutcome::Unsupported => {
            unreachable!("expect_unsupported=false never yields CompileOutcome::Unsupported")
        }
    };

    // The round-trip teeth: IR → {clif,cgif,xcl} → IR round-trip + all-three-edge cross-dialect.
    gmeow_logic_compile::cl_roundtrip::assert_all_dialects_isomorphic(&program)
        .map_err(|e| prefix(format!("CL dialect round-trip failed: {e}")))?;

    // Pin the canonical-fixpoint dialect renderings the round-trip validated.
    let dialect_texts = gmeow_logic_compile::cl_roundtrip::dialect_fixpoint_projections(&program)
        .map_err(|e| prefix(format!("CL dialect projection failed: {e}")))?;

    let mut text = BTreeMap::new();
    let mut cl_dialects = serde_json::Map::new();
    for (dialect, content) in dialect_texts {
        text.insert(dialect.to_string(), content);
        cl_dialects.insert(
            dialect.to_string(),
            serde_json::json!({ "round_trip": "pass" }),
        );
    }
    cl_dialects.insert("cross_dialect".to_string(), serde_json::json!("pass"));

    // A cl-roundtrip case gates NO RDF projection goldens (only the dialect texts), and
    // its `expected/projections/` dir DOES exist (it holds gmeow.{clif,cgif,xcl}), so the
    // RDF compare loop runs. Leave the RDF map EMPTY (not RDF_TARGETS→"") so each target's
    // `produced` is `None` and the loop skips it — an empty-string entry would instead be
    // read as "produced but golden missing" and hard-fail. (empty_outputs can safely use
    // RDF_TARGETS→"" only because an expect_unsupported case has no projections/ dir.)
    let rdf = BTreeMap::new();

    Ok(CaseOutputs {
        case_id: case_id.to_string(),
        materialized_nquads: String::new(),
        // A CL round-trip case does not materialize, so there are no per-quad stamps.
        materialized_quad_status: serde_json::Value::Array(Vec::new()),
        projections: ProjectionOutputs {
            rdf,
            report_turtle: String::new(),
            ledger: serde_json::json!({}),
            text,
            path_projections: Vec::new(),
            // A cl-roundtrip case authors no validation shapes and gates none of the shape goldens.
            shacl_core: String::new(),
            shex: String::new(),
            residue: empty_shape_residue(),
        },
        explanations: Vec::new(),
        verdicts: serde_json::json!({}),
        certification: serde_json::json!({}),
        budget_status: "ok".to_string(),
        incomplete: false,
        // A CL round-trip case does not materialize, so no governor ran.
        frontier: gmeow_logic::query_ir::CompletionFrontier::empty(),
        answers: BTreeMap::new(),
        // A lossless round-trip is Exact by construction (the gate above proved it).
        preservation: serialize::preservation_to_json(&PreservationClaim::exact()),
        correspondence_gates: None,
        cl_dialects: Some(serde_json::Value::Object(cl_dialects)),
    })
}

/// The empty [`CaseOutputs`] an `expect_unsupported` case returns: the program is
/// never evaluated, so there are no quads, projections, verdicts, or answers. The
/// diff phase finds no goldens to compare (the case carries no `expected/` tree),
/// so the case passes purely on the verified unsupported verdict.
fn empty_outputs(case_id: String) -> CaseOutputs {
    let mut rdf = BTreeMap::new();
    for target in RDF_TARGETS {
        rdf.insert(target.to_string(), String::new());
    }
    let mut text = BTreeMap::new();
    for target in ["datalog", "n3"] {
        text.insert(target.to_string(), String::new());
    }
    CaseOutputs {
        case_id,
        materialized_nquads: String::new(),
        // An unsupported case is never evaluated, so there are no per-quad stamps.
        materialized_quad_status: serde_json::Value::Array(Vec::new()),
        projections: ProjectionOutputs {
            rdf,
            report_turtle: String::new(),
            ledger: serde_json::json!({}),
            text,
            path_projections: Vec::new(),
            // An unsupported case is never evaluated: no shapes, no shape goldens gated.
            shacl_core: String::new(),
            shex: String::new(),
            residue: empty_shape_residue(),
        },
        explanations: Vec::new(),
        verdicts: serde_json::json!({}),
        certification: serde_json::json!({}),
        budget_status: "ok".to_string(),
        incomplete: false,
        // The program was never evaluated, so no governor ran.
        frontier: gmeow_logic::query_ir::CompletionFrontier::empty(),
        answers: BTreeMap::new(),
        // The case was refused as unsupported and never evaluated — disclose
        // `{unsupported}` (the legalization floor), never a false `{exact}` that would
        // hide the refusal from a consumer reading `CaseOutputs.preservation`.
        preservation: serialize::preservation_to_json(&PreservationClaim::unsupported()),
        // No program was compiled, so no correspondence gates ran.
        correspondence_gates: None,
        // Not a CL round-trip case.
        cl_dialects: None,
    }
}

/// Run one `verdict_mode = consistency` case.
///
/// External entailment/SZS corpora are lowered into a world-scoped RDF EDB
/// (`input.nq`) and decided by the native DL consistency path
/// ([`gmeow_logic::reason::dl_consistency`]) — the verdict-only entry point that folds
/// from the SAME shared closure as [`gmeow_logic::reason::reason_all`], so the
/// two can never disagree. The per-world verdict is `inconsistent` for any world bearing a
/// populated `owl:Nothing` clash (an [`InconsistencyWitness`]), else `consistent`.
/// No compile / certify / materialize / projection / answer artifacts are produced
/// (a consistency case carries only its `expected/verdicts.json` golden).
fn run_consistency_case(
    case_id: &str,
    case_dir: &Path,
    consistency: &impl Fn(&Path) -> gmeow_errors::Result<crate::consistency::Observation>,
) -> gmeow_errors::Result<CaseOutputs> {
    let prefix = |msg: String| run_fail(format!("case {case_id}: {msg}"));

    // The EDB is the world-scoped N-Quads `input.nq` (hard-fail if absent — a
    // consistency case has no other input the DL path can read).
    let input_nq_path = case_dir.join("input.nq");
    if !input_nq_path.exists() {
        return Err(prefix(
            "verdict_mode=consistency requires input.nq (the world-scoped RDF EDB)".to_string(),
        ));
    }
    let observation = consistency(&input_nq_path)?;
    let verdict = &observation.verdict;

    // Zero-defer: a consistency case MUST be genuinely decided by the native
    // path. A non-empty `gaps` means a construct is present that the native DL path
    // cannot honestly decide — refuse rather than emit a dishonest verdict.
    if !verdict.gaps.is_empty() {
        let gaps: Vec<&str> = verdict.gaps.iter().map(|g| g.code.as_str()).collect();
        return Err(prefix(format!(
            "verdict_mode=consistency case has undecided native DL construct gap(s) {gaps:?} — \
             the engine cannot honestly decide it (zero-defer violation; route heavy corpora to \
             the Lane-B classic-cross-check instead)"
        )));
    }

    // Counts and complete native verdict belong to the same producer observation.
    let world_counts = &observation.world_counts;
    let inconsistent_worlds: BTreeSet<String> = verdict
        .inconsistencies
        .iter()
        .map(|w| w.world.clone())
        .collect();

    // Hard-fail (no-optionality): the emitted verdict iterates `world_counts`
    // (worlds present in the EDB), so every inconsistent world MUST appear there — else
    // `build_verdicts` would silently omit its `inconsistent` status. The seed fixtures'
    // clash worlds all carry EDB quads, so this is a latent-invariant guard: an
    // inference-only inconsistent world fails loudly here rather than vanishing.
    let missing: Vec<&String> = inconsistent_worlds
        .iter()
        .filter(|w| !world_counts.contains_key(*w))
        .collect();
    if !missing.is_empty() {
        return Err(prefix(format!(
            "native DL reported inconsistency for world(s) {missing:?} absent from the EDB world \
             set {:?} — the per-world verdict would silently drop them (an inconsistency must \
             attach to a world present in input.nq)",
            world_counts.keys().collect::<Vec<_>>()
        )));
    }

    let verdicts = serialize::build_verdicts(world_counts, |world| {
        if inconsistent_worlds.contains(world) {
            VerdictStatus::Inconsistent
        } else {
            VerdictStatus::Consistent
        }
    });

    let mut out = empty_outputs(case_id.to_string());
    out.verdicts = verdicts;
    Ok(out)
}

/// Read an optional sibling file, returning the empty string when absent.
fn read_optional(case_dir: &Path, name: &str) -> gmeow_errors::Result<String> {
    let path = case_dir.join(name);
    if path.exists() {
        std::fs::read_to_string(&path).map_err(|e| run_fail(format!("cannot read {name}: {e}")))
    } else {
        Ok(String::new())
    }
}

/// Strip one outer layer of N3 angle brackets from a term string.
fn bare_iri(term: &str) -> String {
    let b = term.as_bytes();
    if b.len() >= 2 && b[0] == b'<' && b[b.len() - 1] == b'>' {
        term[1..term.len() - 1].to_string()
    } else {
        term.to_string()
    }
}

/// Default (non-foundation) materialization: the profile-routed chase. Returns the
/// quads, the aggregate budget status / incomplete flag, and the preservation
/// judgment disclosing any derivation rules the routing could not evaluate.
fn materialize_default(
    case_id: &str,
    program: &gmeow_logic_compile::ir::LogicProgram,
    input_nq: &str,
    profile: &Profile,
) -> gmeow_errors::Result<(
    Vec<RunnerQuad>,
    String,
    bool,
    PreservationClaim,
    gmeow_logic::query_ir::CompletionFrontier,
)> {
    let budget = profile.budget_params.clone().unwrap_or_default();
    if budget.time_ms.is_some() {
        return Err(run_fail(format!(
            "case {case_id}: budget_params.time_ms is unsupported: native materialization uses deterministic derivation-step budgets"
        )));
    }
    let max_steps = match (budget.max_rule_firings, budget.max_answers) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    let dataset = purrdf::parse_dataset(input_nq.as_bytes(), "application/n-quads", None)
        .map_err(|e| run_fail(format!("case {case_id}: input.nq RDF parse failed: {e}")))?;
    let declared_profile =
        gmeow_logic_compile::ir::SemanticProfileId::from_local(&profile.semantic_profile)
            .ok_or_else(|| {
                run_fail(format!(
                    "case {case_id}: unsupported materialization profile {}",
                    profile.semantic_profile
                ))
            })?;
    let derived = materialize_program(
        program,
        dataset.as_ref(),
        MaterializationLimits { max_steps },
        Some(declared_profile),
    )
    .map_err(|e| run_fail(format!("case {case_id}: materialize failed: {e}")))?;

    let preservation = derived.preservation;
    let frontier = derived.frontier;
    let exhausted = derived
        .quads
        .iter()
        .any(|q| q.budget_status == BudgetStatus::Exhausted);
    let quads = derived
        .quads
        .into_iter()
        .map(|dq| RunnerQuad {
            modal_evaluation: None,
            graph: dq.graph.clone(),
            subject: bare_iri(&gmeow_logic::provenance::term_display(&dq.subject)),
            predicate: dq.predicate.clone(),
            obj: gmeow_logic::provenance::term_display(&dq.object),
            derivation_id: dq.derivation_id.as_str().to_string(),
            rule_iri: dq.rule_iri,
            source_quad_ids: dq.source_quad_ids,
            // The frontier-aware per-quad stamp: a saturated-stratum quad stays `ok` even
            // under an exhausted run; only cut/unreached-stratum quads carry `exhausted`.
            budget_status: dq.budget_status.as_str().to_string(),
        })
        .collect();

    let status = if exhausted { "exhausted" } else { "ok" };
    Ok((quads, status.to_string(), exhausted, preservation, frontier))
}

/// Foundation-lowering materialization via the native OntoUML evaluator. The
/// foundation evaluator has no budget governor, so a declared `budget_params` is
/// a hard failure.
fn materialize_foundation(
    case_id: &str,
    input_nq: &str,
    profile: &Profile,
) -> gmeow_errors::Result<(
    Vec<RunnerQuad>,
    String,
    bool,
    PreservationClaim,
    gmeow_logic::query_ir::CompletionFrontier,
)> {
    if profile.budget_params.is_some() {
        return Err(run_fail(format!(
            "case {case_id}: foundation_lowering cases cannot declare budget_params — \
             the native foundation evaluator has no budget governor"
        )));
    }
    // Foundation worlds are flat named graphs; the profile is stamped PositiveHorn
    // to match the committed goldens. (POSITIVE_HORN_PROFILE documents that intent;
    // the native evaluator stamps the same value.)
    let _ = POSITIVE_HORN_PROFILE;

    let policy = AntiRigidityPolicy::from_str(&profile.anti_rigidity_policy)
        .map_err(|e| run_fail(format!("case {case_id}: invalid anti_rigidity_policy: {e}")))?;

    let quads = if input_nq.trim().is_empty() {
        Vec::new()
    } else {
        let store = WorldStore::new();
        store.load_nquads(input_nq).map_err(|e| {
            run_fail(format!(
                "case {case_id}: foundation N-Quads parse failed: {e}"
            ))
        })?;
        let fq = foundation_evaluate(&store, policy)
            .map_err(|e| run_fail(format!("case {case_id}: foundation evaluation failed: {e}")))?;
        fq.into_iter()
            .map(|q| RunnerQuad {
                modal_evaluation: q.modal_evaluation,
                graph: q.graph,
                // Foundation subjects/objects are already bare / N3 respectively.
                subject: q.subject,
                predicate: q.predicate,
                obj: q.object,
                derivation_id: q.derivation_id,
                rule_iri: q.rule_iri,
                source_quad_ids: q.source_quad_ids,
                // The foundation chase runs to completion (no governor) ⇒ every quad `ok`.
                budget_status: BudgetStatus::Ok.as_str().to_string(),
            })
            .collect()
    };
    // The foundation evaluator runs the stratified chase to completion — faithful,
    // nothing dropped, so the materialization is exact.
    Ok((
        quads,
        "ok".to_string(),
        false,
        PreservationClaim::exact(),
        // The foundation evaluator runs the stratified chase to completion outside the
        // native semi-naive governor, so it exposes no stratum frontier.
        gmeow_logic::query_ir::CompletionFrontier::empty(),
    ))
}

/// Teleology-lowering materialization via the native canonical-process teleology evaluator.
///
/// Mirrors [`materialize_foundation`] exactly: the teleology evaluator has no budget
/// governor and needs no rule-program input, so a declared `budget_params` is a hard failure,
/// and the only input it reads is the world-scoped `input.nq`. It runs ALL applicable
/// teleology computations (goal-expression evaluation, plan-success classification,
/// deontic obligation/prohibition, serialization-anomaly detection, and the
/// satisfiedBy⟷GoalEvaluation bridge) over the worlds the EDB carries, mapping each
/// [`gmeow_logic::teleology::TeleologyQuad`] → [`RunnerQuad`] (the mapping is identical
/// to the foundation one — the two quad types are shape-identical).
fn materialize_teleology(
    case_id: &str,
    input_nq: &str,
    profile: &Profile,
) -> gmeow_errors::Result<(
    Vec<RunnerQuad>,
    String,
    bool,
    PreservationClaim,
    gmeow_logic::query_ir::CompletionFrontier,
)> {
    if profile.budget_params.is_some() {
        return Err(run_fail(format!(
            "case {case_id}: teleology_lowering cases cannot declare budget_params — \
             the native teleology evaluator has no budget governor"
        )));
    }
    let (quads, claim) = if input_nq.trim().is_empty() {
        (Vec::new(), PreservationClaim::exact())
    } else {
        let store = WorldStore::new();
        store.load_nquads(input_nq).map_err(|e| {
            run_fail(format!(
                "case {case_id}: teleology N-Quads parse failed: {e}"
            ))
        })?;
        let (tq, claim) = teleology_evaluate(&store)
            .map_err(|e| run_fail(format!("case {case_id}: teleology evaluation failed: {e}")))?;
        let quads: Vec<RunnerQuad> = tq
            .into_iter()
            .map(|q| RunnerQuad {
                modal_evaluation: None,
                graph: q.graph,
                // Teleology subjects/objects are already bare / N3 respectively
                // (shape-identical to FoundationQuad).
                subject: q.subject,
                predicate: q.predicate,
                obj: q.object,
                derivation_id: q.derivation_id,
                rule_iri: q.rule_iri,
                source_quad_ids: q.source_quad_ids,
                // The teleology evaluator runs to completion (no governor) ⇒ every quad `ok`.
                budget_status: BudgetStatus::Ok.as_str().to_string(),
            })
            .collect();
        (quads, claim)
    };
    // The production teleology claim carries the runtime preservation judgment:
    // exact when no satisfiedBy edge was generated; SoundUnder (naming the dropped
    // GoalEvaluation factored axes) when the forward bridge fired.
    Ok((
        quads,
        "ok".to_string(),
        false,
        claim,
        // The teleology evaluator has no budget governor, so it exposes no frontier.
        gmeow_logic::query_ir::CompletionFrontier::empty(),
    ))
}

/// Produce one explanation skeleton per quad. Asserted quads get a trivial
/// depth-0 explanation.
fn run_explanations(
    case_id: &str,
    quads: &[RunnerQuad],
) -> gmeow_errors::Result<Vec<ExplanationOut>> {
    if quads.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<Row> = quads
        .iter()
        .map(|q| Row {
            modal_evaluation: q.modal_evaluation.clone(),
            graph: q.graph.clone(),
            subject: bare_iri(&q.subject),
            predicate: q.predicate.clone(),
            obj: q.obj.clone(),
            derivation_id: q.derivation_id.clone(),
            rule_iri: q.rule_iri.clone(),
            source_quad_ids: q.source_quad_ids.clone(),
        })
        .collect();
    let explanations =
        explain_all(&rows).map_err(|e| run_fail(format!("case {case_id}: explain failed: {e}")))?;
    Ok(explanations
        .into_iter()
        .map(|e| {
            let markdown = gmeow_logic::explain::render_markdown(&e);
            ExplanationOut {
                target_quad_reifier: e.target_quad_reifier,
                cited_iris: e.cited_iris,
                markdown,
            }
        })
        .collect())
}

/// Resolve every `queries/*.logic` backward goal over the materialized EDB.
/// Empty map when there is no `queries/` directory.
fn resolve_answers(
    case_id: &str,
    case_dir: &Path,
    world_nquads: &str,
    profile_str: &str,
    budget: &Option<BudgetParams>,
) -> gmeow_errors::Result<BTreeMap<String, serde_json::Value>> {
    let queries_dir = case_dir.join("queries");
    if !queries_dir.is_dir() {
        return Ok(BTreeMap::new());
    }
    let mut query_files: Vec<std::path::PathBuf> = std::fs::read_dir(&queries_dir)
        .map_err(|e| run_fail(format!("case {case_id}: cannot read queries/: {e}")))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|e| run_fail(format!("case {case_id}: cannot inspect queries/: {e}")))?
        .into_iter()
        .filter(|p| p.extension().is_some_and(|x| x == "logic"))
        .collect();
    query_files.sort();
    if query_files.is_empty() {
        return Ok(BTreeMap::new());
    }

    let store = WorldStore::new();
    store
        .load_nquads(world_nquads)
        .map_err(|e| run_fail(format!("case {case_id}: query failed: {e}")))?;
    let worlds = store.worlds();
    if worlds.len() != 1 {
        return Err(run_fail(format!(
            "case {case_id}: world not given and the store has {} named graphs (need exactly 1)",
            worlds.len()
        )));
    }
    let world = worlds.into_iter().next().expect("len == 1");
    let query_world = QueryWorld {
        store: &store,
        world: &world,
        profile: profile_str,
        foreign: std::cell::OnceCell::new(),
    };
    let max_answers = budget.as_ref().and_then(|b| b.max_answers);
    let max_steps = budget.as_ref().and_then(|b| b.max_steps);
    let mut answers = BTreeMap::new();
    for qfile in query_files {
        let stem = qfile
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| {
                run_fail(format!(
                    "case {case_id}: bad query filename {}",
                    qfile.display()
                ))
            })?
            .to_string();
        let qtext = std::fs::read_to_string(&qfile)
            .map_err(|e| run_fail(format!("case {case_id}: cannot read query {stem}: {e}")))?;
        let answer = resolve_query(case_id, &query_world, &qtext, max_answers, max_steps)?;
        answers.insert(stem, answer);
    }
    Ok(answers)
}

/// Invocation-local immutable world and its lazily shared native fact snapshot.
/// Counterfactual/probabilistic queries keep their own semantic path; selecting
/// them never forces the ordinary backward-query snapshot.
struct QueryWorld<'store> {
    store: &'store WorldStore,
    world: &'store str,
    profile: &'store str,
    foreign: std::cell::OnceCell<WorldFactSnapshot>,
}

/// Resolve a single `.logic` backward goal (mirrors the `gmeow_logic.query` PyO3
/// routing: probabilistic / counterfactual / dispatched). Returns
/// `{"bindings": [...], "status": "..."}`.
fn resolve_query(
    case_id: &str,
    query_world: &QueryWorld<'_>,
    query_text: &str,
    max_answers: Option<u64>,
    max_steps: Option<u64>,
) -> gmeow_errors::Result<serde_json::Value> {
    let err = |msg: String| run_fail(format!("case {case_id}: query failed: {msg}"));

    let store = query_world.store;
    let world = query_world.world;
    let profile_str = query_world.profile;

    let program = parse_query_program(query_text).map_err(|e| err(e.message().to_owned()))?;
    let max_answers_usize = max_answers.map(|n| n as usize);

    // Probabilistic profile: weighted model counting; each binding carries a
    // `probability`. This is the only path that emits that key.
    if gmeow_logic::profile_gate::is_probabilistic_profile(profile_str) {
        let answer =
            gmeow_logic::probabilistic::evaluate(store, world, &program, profile_str, None)
                .map_err(|e| err(e.message().to_owned()))?;
        let bindings: Vec<serde_json::Value> = answer
            .bindings
            .iter()
            .map(|b| {
                let mut obj = serde_json::Map::new();
                for (var, val) in &b.vars {
                    obj.insert(var.clone(), serde_json::Value::String(val.clone()));
                }
                obj.insert("probability".to_string(), serde_json::json!(b.probability));
                serde_json::Value::Object(obj)
            })
            .collect();
        return Ok(serde_json::json!({
            "bindings": bindings,
            "status": answer.status_str(),
            "preservation": serialize::preservation_to_json(&answer.result.preservation),
        }));
    }

    let budget = Budget {
        max_answers: max_answers_usize,
        max_steps,
    };

    // Counterfactual vs plain backward goal. Both carry a preservation
    // claim disclosing what the target evaluated; the dispatched backward goal also
    // carries the native governor's completion frontier.
    let (bindings_vec, status, preservation, frontier): (
        Vec<gmeow_logic::query_ir::Binding>,
        String,
        gmeow_logic::result::PreservationClaim,
        gmeow_logic::query_ir::CompletionFrontier,
    ) = if gmeow_logic::counterfactual::is_counterfactual(&program) {
        let depth = program
            .counterfactual
            .as_ref()
            .and_then(|c| c.depth_budget)
            .unwrap_or(gmeow_logic::counterfactual::DEFAULT_DEPTH_BUDGET);
        let mut cf = gmeow_logic::counterfactual::construct_and_resolve(
            store,
            &program,
            profile_str,
            &budget,
            depth,
            None,
        )
        .map_err(|e| err(e.message().to_owned()))?;
        let status = cf.status_str().to_string();
        let preservation = cf.result.preservation.clone();
        // The counterfactual constructor runs its own bounded search, not the native
        // semi-naive governor, so it exposes no stratum frontier.
        (
            std::mem::take(&mut cf.bindings),
            status,
            preservation,
            gmeow_logic::query_ir::CompletionFrontier::empty(),
        )
    } else {
        if query_world.foreign.get().is_none() {
            let snapshot = WorldFactSnapshot::from_world(store, world, profile_str)
                .map_err(|e| err(e.message().to_owned()))?;
            assert!(
                query_world.foreign.set(snapshot).is_ok(),
                "single-threaded case query initialization"
            );
        }
        let foreign = query_world
            .foreign
            .get()
            .expect("initialized native world snapshot");
        let answer =
            gmeow_logic::dispatch::dispatch_query(foreign, world, &program, profile_str, &budget)
                .map_err(|e| err(e.message().to_owned()))?;
        let preservation = answer.preservation.clone();
        (
            answer.bindings,
            answer.status.as_str().to_string(),
            preservation,
            answer.frontier,
        )
    };

    let bindings: Vec<serde_json::Value> = bindings_vec
        .iter()
        .map(|b| {
            let mut obj = serde_json::Map::new();
            for (var, val) in b {
                obj.insert(var.clone(), serde_json::Value::String(val.clone()));
            }
            serde_json::Value::Object(obj)
        })
        .collect();
    let mut answer_json = serde_json::json!({
        "bindings": bindings,
        "status": status,
        "preservation": serialize::preservation_to_json(&preservation),
    });
    // Surface the completion frontier ONLY when the query declared a step budget
    // (`max_steps`) — the frontier answers "which strata completed" precisely when a
    // step budget could truncate the backward search. A pure `max_answers` cap
    // (post-fixpoint truncation) or an ungoverned goal adds no frontier key.
    if max_steps.is_some() {
        let obj = answer_json
            .as_object_mut()
            .expect("json! object is always an object");
        obj.insert(
            "strata_completed".to_string(),
            serde_json::json!(frontier.completed),
        );
        obj.insert(
            "strata_total".to_string(),
            serde_json::json!(frontier.total),
        );
        obj.insert(
            "saturated".to_string(),
            serde_json::json!(frontier.saturated_preds.iter().collect::<Vec<_>>()),
        );
    }
    Ok(answer_json)
}

/// Whether a quad is an asserted (EDB) input fact rather than a derived one.
/// (Currently unused by the comparison path; retained for parity with the Python
/// derived/input split and potential future verdicts.)
#[allow(dead_code)]
fn is_asserted(quad: &RunnerQuad) -> bool {
    quad.rule_iri == ASSERT_RULE_IRI
}

// Authored cases execute only in the explicit optimized producer. Its complete
// observations are graded against the committed goldens by the pipeline's
// authenticated conformance consumer. Local tests below construct synthetic inputs.
//
// The diagnostic-gating firewall IS unit-tested below because its
// negative branches (a supported contract under `expect_unsupported`, an
// un-declared compile error) are not exercisable through the committed corpus —
// every committed `expected/`-bearing case is a supported preset, so a smoke test
// over a tiny synthetic case dir is the only way to pin those refusals.

#[path = "run.gating_tests.rs"]
#[cfg(test)]
mod gating_tests;
