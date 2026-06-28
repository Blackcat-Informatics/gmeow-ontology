// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-case orchestration (`run_case`).
//!
//! Drives the `gmeow_logic` native cores for one conformance case and assembles a
//! typed [`CaseOutputs`] by calling the SAME native functions the PyO3 surface wraps
//! (compile → certify → materialize+explain / foundation → answers). There is no
//! PyO3, no Python, and no second engine in this path — the harness is a second
//! *caller* of the engine cores, so its artifacts are identical by construction
//! (the retired Python `logic_runner.run` this replaced was removed in #727).
//!
//! Witnesses (`witnesses.json`) are intentionally NOT produced: the diff phase
//! never compared them — they are a bless-only side file — so omitting them
//! changes no gate verdict.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use gmeow_logic::explain::{explain_all, Row};
use gmeow_logic::foundation::{evaluate as foundation_evaluate, AntiRigidityPolicy};
use gmeow_logic::materialize::materialize_routed;
use gmeow_logic::query_ir::{parse_query_program, Budget};
use gmeow_logic::result::PreservationClaim;
use gmeow_logic::seam::{BudgetStatus, WorldStoreForeign};
use gmeow_logic::store::WorldStore;
use gmeow_logic::teleology::materialize_teleology as teleology_evaluate;
use gmeow_logic_compile::frontend::{parse_logic_str, Diagnostic, Severity};
use gmeow_logic_compile::projections::compile_program;

use crate::profile::{BudgetParams, Profile, VerdictMode};
use crate::serialize::VerdictStatus;
use crate::{profile, serialize};

/// A single materialized quad in the runner's flat string form (mirrors the
/// Python `DerivedQuad` surface the downstream consumers read). The subject is a
/// BARE IRI; the object is in N3 form (`<iri>` or a literal).
#[derive(Debug, Clone)]
pub struct RunnerQuad {
    pub graph: String,
    pub subject: String,
    pub predicate: String,
    pub obj: String,
    pub derivation_id: String,
    pub rule_iri: String,
    pub source_quad_ids: Vec<String>,
}

/// One explanation skeleton, keyed by its target quad reifier (the match key the
/// `expected/explanation/*.md` goldens use).
#[derive(Debug, Clone)]
pub struct ExplanationOut {
    pub target_quad_reifier: String,
    pub cited_iris: BTreeSet<String>,
    /// Full Markdown rendering of this explanation, suitable for writing to
    /// `expected/explanation/{hash}.md`.
    pub markdown: String,
}

/// One path-shape projection entry in the conformance output.
#[derive(Debug, Clone)]
pub struct PathProjectionOut {
    /// IRI of the projected `logic:PathShape`.
    pub shape_iri: String,
    /// The serialized extended SPARQL property path.
    pub property_path: String,
    /// The depth-bounded Datalog rule scheme (native-engine syntax).
    pub datalog: String,
}

/// The projection artifacts for one case.
#[derive(Debug, Clone)]
pub struct ProjectionOutputs {
    /// The four RDF targets (`owl-dl`, `owl-el`, `gufo`, `canonical-rdf12`) as
    /// Turtle, compared by graph-isomorphism.
    pub rdf: BTreeMap<String, String>,
    /// The preservation report as Turtle (graph-isomorphism comparison).
    pub report_turtle: String,
    /// The preservation ledger JSON (`{target: {preservation, complexity, lossy_drops}}`).
    pub ledger: serde_json::Value,
    /// Plain-text projections (`datalog`, `n3`, `nemo`) — kept for bless; not diffed.
    pub text: BTreeMap<String, String>,
    /// Per-shape property-path projections (`logic:PathShape` → SPARQL + Datalog).
    /// Empty when the program declares no path shapes — never absent.
    pub path_projections: Vec<PathProjectionOut>,
}

/// Everything one case run produces, ready for `diff_case` / bless.
#[derive(Debug, Clone)]
pub struct CaseOutputs {
    pub case_id: String,
    pub materialized_nquads: String,
    pub projections: ProjectionOutputs,
    pub explanations: Vec<ExplanationOut>,
    pub verdicts: serde_json::Value,
    pub certification: serde_json::Value,
    pub budget_status: String,
    pub incomplete: bool,
    /// `{query_stem: {"bindings": [...], "status": "...", "preservation": {...}}}` for
    /// each `queries/*.logic`.
    pub answers: BTreeMap<String, serde_json::Value>,
    /// The materialization's runtime preservation judgment (downstream disclosure):
    /// `{polarities, unsupported_constructs}`. `{exact}` for the faithful chase /
    /// foundation paths; `{sound-under}` naming the dropped rules for the
    /// non-stratifiable EDB-echo path. Distinct from the compile-time projection
    /// ledger in `projections.ledger`.
    pub preservation: serde_json::Value,
}

/// The four RDF projection targets compared by graph-isomorphism.
const RDF_TARGETS: [&str; 4] = ["owl-dl", "owl-el", "gufo", "canonical-rdf12"];

/// The sentinel rule IRI marking an asserted (EDB) input fact.
const ASSERT_RULE_IRI: &str = "https://blackcatinformatics.ca/logic/assert";

/// The profile IRI foundation cases materialize under (matches the native
/// evaluator's stamped profile and the committed goldens).
const POSITIVE_HORN_PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";

/// Run one conformance case end to end, producing its [`CaseOutputs`].
///
/// # Errors
/// Returns a human-readable error string (prefixed with the case id) on any
/// malformed input, profile, or engine failure — hard-fail, no silent skip.
pub fn run_case(case_dir: &Path) -> Result<CaseOutputs, String> {
    let case_id = crate::paths::case_id(case_dir);
    let prefix = |msg: String| format!("case {case_id}: {msg}");

    // ── Profile ──────────────────────────────────────────────────────────────
    let profile_text = std::fs::read_to_string(case_dir.join("profile.json"))
        .map_err(|e| prefix(format!("cannot read profile.json: {e}")))?;
    let profile_value: serde_json::Value = serde_json::from_str(&profile_text)
        .map_err(|e| prefix(format!("cannot parse profile.json: {e}")))?;
    let profile = profile::parse_profile(&case_id, &profile_value)?;

    // ── Consistency mode (#753) ───────────────────────────────────────────────
    // External entailment/SZS cases reason over their RDF EDB through the native
    // DL consistency path, NOT the logic-compile/materialize chase. Branch BEFORE
    // reading/compiling `input.logic.ttl` (which a consistency case does not use).
    if profile.verdict_mode == VerdictMode::Consistency {
        return run_consistency_case(&case_id, case_dir);
    }

    // ── Compile (frontend → IR → projections + nemo rules + ledger) ──────────
    let source = std::fs::read_to_string(case_dir.join("input.logic.ttl"))
        .map_err(|e| prefix(format!("cannot read input.logic.ttl: {e}")))?;
    // `parse_logic_str` returns `Err` only on a hard Turtle PARSE failure; a
    // semantic `Severity::Error` diagnostic (e.g. an UNSUPPORTED_CONTRACT forbidden
    // facet combination) is carried INSIDE the diagnostics vec with `Ok((..))`. The
    // harness must respect those errors rather than silently proceed to evaluate —
    // otherwise the "unsupported is a hard stop" guarantee is unpinned (#767 Gap 2).
    let (program, diagnostics) = parse_logic_str(&source, None)
        .map_err(|e| prefix(format!("compile parse failed: {}", e.0)))?;

    // ── Unsupported-contract firewall (#767 Gap 2) ────────────────────────────
    // An `expect_unsupported` case asserts the contract authors a forbidden facet
    // combination: require the compile to have flagged it (UNSUPPORTED_CONTRACT
    // Severity::Error) and short-circuit WITHOUT evaluating/certifying/materializing.
    if profile.expect_unsupported {
        if !has_unsupported_contract_error(&diagnostics) {
            return Err(prefix(format!(
                "profile.json declares \"expect_unsupported\": true but the compile produced \
                 no UNSUPPORTED_CONTRACT Severity::Error — the engine accepted the contract. \
                 Diagnostics: {diagnostics:?}"
            )));
        }
        // The program must not proceed: return empty outputs so the diff phase sees
        // no goldens to compare (an expect_unsupported case carries no expected/ tree).
        return Ok(empty_outputs(case_id));
    }

    // A non-`expect_unsupported` case that nonetheless emits ANY Severity::Error
    // diagnostic is a silent-run hole: hard-fail so it can never evaluate as if the
    // contract were sound. (All committed supported presets compile clean.)
    if let Some(first) = first_error(&diagnostics) {
        return Err(prefix(format!(
            "compile emitted a Severity::Error diagnostic but the case does not declare \
             \"expect_unsupported\": true — refusing to evaluate an unsound program. \
             First error [{}]: {}",
            first.code, first.message
        )));
    }

    let arts = compile_program(&program).map_err(|e| prefix(format!("compile failed: {e}")))?;
    let nemo_rules = arts.nemo_rules.clone();

    // ── Static certification against the declared profile ────────────────────
    let verdict = gmeow_logic::certify::certify(&nemo_rules, &profile.semantic_profile)
        .map_err(|e| prefix(format!("certify failed: {e}")))?;
    let certification = serialize::certification_to_json(&verdict);

    // ── Materialization (+ explanations) ─────────────────────────────────────
    let input_nq = read_optional(case_dir, "input.nq")?;
    let (quads, budget_status, incomplete, mat_preservation) = if profile.foundation_lowering {
        materialize_foundation(&case_id, &input_nq, &profile)?
    } else if profile.teleology_lowering {
        materialize_teleology(&case_id, &input_nq, &profile)?
    } else {
        materialize_default(&case_id, &nemo_rules, &input_nq, &profile)?
    };
    let explanations = run_explanations(&case_id, &quads)?;

    // ── N-Quads serialization + downstream artifacts ─────────────────────────
    let materialized_nquads = serialize::materialized_to_nquads(&quads);
    // Materialization-mode status: every materializing world is `consistent`,
    // EXCEPT when the budget governor exhausted the chase — then the run is
    // `incomplete` (the external `Unknown`/budget-tripped branch, #753). A clean
    // (non-exhausted) run reproduces the pre-#753 `consistent` golden byte-for-byte.
    let mat_status = if incomplete {
        VerdictStatus::Incomplete
    } else {
        VerdictStatus::Consistent
    };
    let world_counts = serialize::count_worlds(&quads);
    let verdicts = serialize::build_verdicts(&world_counts, |_| mat_status);

    // ── Backward goals (#504) ────────────────────────────────────────────────
    let answers = resolve_answers(
        &case_id,
        case_dir,
        &materialized_nquads,
        profile.query_profile(),
        &profile.budget_params,
    )?;

    // ── Projections repackaged for the diff ──────────────────────────────────
    let mut rdf = BTreeMap::new();
    rdf.insert("owl-dl".to_string(), arts.owl_dl.clone());
    rdf.insert("owl-el".to_string(), arts.owl_el.clone());
    rdf.insert("gufo".to_string(), arts.gufo.clone());
    rdf.insert("canonical-rdf12".to_string(), arts.canonical_rdf12.clone());
    debug_assert!(RDF_TARGETS.iter().all(|t| rdf.contains_key(*t)));
    let mut text = BTreeMap::new();
    text.insert("datalog".to_string(), arts.datalog.clone());
    text.insert("n3".to_string(), arts.n3.clone());
    text.insert("nemo".to_string(), arts.nemo.clone());

    let path_projections_out: Vec<PathProjectionOut> = arts
        .path_projections
        .iter()
        .map(|pp| PathProjectionOut {
            shape_iri: pp.shape_iri.clone(),
            property_path: pp.property_path.clone(),
            datalog: pp.datalog.clone(),
        })
        .collect();

    let projections = ProjectionOutputs {
        rdf,
        report_turtle: arts.report.clone(),
        ledger: serialize::ledger_to_json(&arts.preservation_ledger),
        text,
        path_projections: path_projections_out,
    };

    Ok(CaseOutputs {
        case_id,
        materialized_nquads,
        projections,
        explanations,
        verdicts,
        certification,
        budget_status,
        incomplete,
        answers,
        preservation: serialize::preservation_to_json(&mat_preservation),
    })
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
    for target in ["datalog", "n3", "nemo"] {
        text.insert(target.to_string(), String::new());
    }
    CaseOutputs {
        case_id,
        materialized_nquads: String::new(),
        projections: ProjectionOutputs {
            rdf,
            report_turtle: String::new(),
            ledger: serde_json::json!({}),
            text,
            path_projections: Vec::new(),
        },
        explanations: Vec::new(),
        verdicts: serde_json::json!({}),
        certification: serde_json::json!({}),
        budget_status: "ok".to_string(),
        incomplete: false,
        answers: BTreeMap::new(),
        // The case was refused as unsupported and never evaluated — disclose
        // `{unsupported}` (the legalization floor), never a false `{exact}` that would
        // hide the refusal from a consumer reading `CaseOutputs.preservation`.
        preservation: serialize::preservation_to_json(&PreservationClaim::unsupported()),
    }
}

/// Run one `verdict_mode = consistency` case (#753).
///
/// External entailment/SZS corpora are lowered into a world-scoped RDF EDB
/// (`input.nq`) and decided by the native DL consistency path
/// ([`gmeow_logic::reason::dl_consistency`]) — the verdict-only entry point that folds
/// from the SAME shared closure as [`gmeow_logic::reason::reason_all`] (#768), so the
/// two can never disagree. The per-world verdict is `inconsistent` for any world bearing a
/// populated `owl:Nothing` clash (an [`InconsistencyWitness`]), else `consistent`.
/// No compile / certify / materialize / projection / answer artifacts are produced
/// (a consistency case carries only its `expected/verdicts.json` golden).
fn run_consistency_case(case_id: &str, case_dir: &Path) -> Result<CaseOutputs, String> {
    let prefix = |msg: String| format!("case {case_id}: {msg}");

    // The EDB is the world-scoped N-Quads `input.nq` (hard-fail if absent — a
    // consistency case has no other input the DL path can read).
    let input_nq_path = case_dir.join("input.nq");
    if !input_nq_path.exists() {
        return Err(prefix(
            "verdict_mode=consistency requires input.nq (the world-scoped RDF EDB)".to_string(),
        ));
    }
    let bytes =
        std::fs::read(&input_nq_path).map_err(|e| prefix(format!("cannot read input.nq: {e}")))?;
    let dataset = gmeow_rdf::dataset_from_bytes(&bytes, gmeow_rdf::NativeRdfFormat::NQuads)
        .map_err(|e| prefix(format!("input.nq parse failed: {e}")))?;

    let verdict = gmeow_logic::reason::dl_consistency(dataset.as_ref())
        .map_err(|e| prefix(format!("native DL consistency run failed: {e}")))?;

    // Zero-defer (#753): a consistency case MUST be genuinely decided by the native
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

    // Per-world quad counts come from the EDB; per-world status from the witnesses.
    let store = WorldStore::new();
    store
        .load_dataset(dataset.as_ref())
        .map_err(|e| prefix(format!("EDB world load failed: {e}")))?;
    let mut world_counts: BTreeMap<String, u64> = BTreeMap::new();
    for world in store.worlds() {
        let n = store
            .quads_for_pattern_in_world(&world, None, None, None)
            .len() as u64;
        world_counts.insert(world, n);
    }
    let inconsistent_worlds: BTreeSet<String> = verdict
        .inconsistencies
        .iter()
        .map(|w| w.world.clone())
        .collect();

    // Hard-fail (no-optionality, #753): the emitted verdict iterates `world_counts`
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

    let verdicts = serialize::build_verdicts(&world_counts, |world| {
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
fn read_optional(case_dir: &Path, name: &str) -> Result<String, String> {
    let path = case_dir.join(name);
    if path.exists() {
        std::fs::read_to_string(&path).map_err(|e| format!("cannot read {name}: {e}"))
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
    nemo_rules: &str,
    input_nq: &str,
    profile: &Profile,
) -> Result<(Vec<RunnerQuad>, String, bool, PreservationClaim), String> {
    let budget = profile.budget_params.clone().unwrap_or_default();
    let derived = materialize_routed(
        nemo_rules,
        input_nq,
        budget.max_rule_firings,
        budget.max_answers,
        budget.time_ms,
        Some(&profile.semantic_profile),
    )
    .map_err(|e| format!("case {case_id}: materialize failed: {e}"))?;

    let preservation = derived.preservation;
    let exhausted = derived
        .quads
        .iter()
        .any(|q| q.budget_status == BudgetStatus::Exhausted);
    let quads = derived
        .quads
        .into_iter()
        .map(|dq| RunnerQuad {
            graph: dq.graph.as_str().to_string(),
            subject: bare_iri(&dq.subject.to_string()),
            predicate: dq.predicate.as_str().to_string(),
            obj: dq.object.to_string(),
            derivation_id: dq.derivation_id.as_str().to_string(),
            rule_iri: dq.rule_iri,
            source_quad_ids: dq.source_quad_ids,
        })
        .collect();

    let status = if exhausted { "exhausted" } else { "ok" };
    Ok((quads, status.to_string(), exhausted, preservation))
}

/// Foundation-lowering materialization via the native OntoUML evaluator. The
/// foundation evaluator has no budget governor, so a declared `budget_params` is
/// a hard failure.
fn materialize_foundation(
    case_id: &str,
    input_nq: &str,
    profile: &Profile,
) -> Result<(Vec<RunnerQuad>, String, bool, PreservationClaim), String> {
    if profile.budget_params.is_some() {
        return Err(format!(
            "case {case_id}: foundation_lowering cases cannot declare budget_params — \
             the native foundation evaluator has no budget governor"
        ));
    }
    // Foundation worlds are flat named graphs; the profile is stamped PositiveHorn
    // to match the committed goldens. (POSITIVE_HORN_PROFILE documents that intent;
    // the native evaluator stamps the same value.)
    let _ = POSITIVE_HORN_PROFILE;

    let policy = AntiRigidityPolicy::from_str(&profile.anti_rigidity_policy)
        .map_err(|e| format!("case {case_id}: invalid anti_rigidity_policy: {e}"))?;

    let quads = if input_nq.trim().is_empty() {
        Vec::new()
    } else {
        let store = WorldStore::new();
        store
            .load_nquads(input_nq)
            .map_err(|e| format!("case {case_id}: foundation N-Quads parse failed: {e}"))?;
        let fq = foundation_evaluate(&store, policy)
            .map_err(|e| format!("case {case_id}: foundation evaluation failed: {e}"))?;
        fq.into_iter()
            .map(|q| RunnerQuad {
                graph: q.graph,
                // Foundation subjects/objects are already bare / N3 respectively.
                subject: q.subject,
                predicate: q.predicate,
                obj: q.object,
                derivation_id: q.derivation_id,
                rule_iri: q.rule_iri,
                source_quad_ids: q.source_quad_ids,
            })
            .collect()
    };
    // The foundation evaluator runs the stratified chase to completion — faithful,
    // nothing dropped, so the materialization is exact.
    Ok((quads, "ok".to_string(), false, PreservationClaim::exact()))
}

/// Teleology-lowering materialization via the native canonical-process teleology evaluator.
///
/// Mirrors [`materialize_foundation`] exactly: the teleology evaluator has no budget
/// governor and needs no nemo rules, so a declared `budget_params` is a hard failure,
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
) -> Result<(Vec<RunnerQuad>, String, bool, PreservationClaim), String> {
    if profile.budget_params.is_some() {
        return Err(format!(
            "case {case_id}: teleology_lowering cases cannot declare budget_params — \
             the native teleology evaluator has no budget governor"
        ));
    }
    let quads = if input_nq.trim().is_empty() {
        Vec::new()
    } else {
        let store = WorldStore::new();
        store
            .load_nquads(input_nq)
            .map_err(|e| format!("case {case_id}: teleology N-Quads parse failed: {e}"))?;
        let tq = teleology_evaluate(&store)
            .map_err(|e| format!("case {case_id}: teleology evaluation failed: {e}"))?;
        tq.into_iter()
            .map(|q| RunnerQuad {
                graph: q.graph,
                // Teleology subjects/objects are already bare / N3 respectively
                // (shape-identical to FoundationQuad).
                subject: q.subject,
                predicate: q.predicate,
                obj: q.object,
                derivation_id: q.derivation_id,
                rule_iri: q.rule_iri,
                source_quad_ids: q.source_quad_ids,
            })
            .collect()
    };
    // The native teleology evaluator classifies/evaluates the given structure and
    // records the result exactly — no lossy projection — so the materialization is
    // an exact preservation claim, mirroring the foundation evaluator.
    Ok((quads, "ok".to_string(), false, PreservationClaim::exact()))
}

/// Produce one explanation skeleton per quad. Asserted quads get a trivial
/// depth-0 explanation.
fn run_explanations(case_id: &str, quads: &[RunnerQuad]) -> Result<Vec<ExplanationOut>, String> {
    if quads.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<Row> = quads
        .iter()
        .map(|q| Row {
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
        explain_all(&rows).map_err(|e| format!("case {case_id}: explain failed: {e}"))?;
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
) -> Result<BTreeMap<String, serde_json::Value>, String> {
    let queries_dir = case_dir.join("queries");
    if !queries_dir.is_dir() {
        return Ok(BTreeMap::new());
    }
    let mut query_files: Vec<std::path::PathBuf> = std::fs::read_dir(&queries_dir)
        .map_err(|e| format!("case {case_id}: cannot read queries/: {e}"))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "logic"))
        .collect();
    query_files.sort();
    if query_files.is_empty() {
        return Ok(BTreeMap::new());
    }

    let max_answers = budget.as_ref().and_then(|b| b.max_answers);
    let mut answers = BTreeMap::new();
    for qfile in query_files {
        let stem = qfile
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("case {case_id}: bad query filename {}", qfile.display()))?
            .to_string();
        let qtext = std::fs::read_to_string(&qfile)
            .map_err(|e| format!("case {case_id}: cannot read query {stem}: {e}"))?;
        let answer = resolve_query(case_id, world_nquads, &qtext, profile_str, max_answers)?;
        answers.insert(stem, answer);
    }
    Ok(answers)
}

/// Resolve a single `.logic` backward goal (mirrors the `gmeow_logic.query` PyO3
/// routing: probabilistic / counterfactual / dispatched). Returns
/// `{"bindings": [...], "status": "..."}`.
fn resolve_query(
    case_id: &str,
    world_nquads: &str,
    query_text: &str,
    profile_str: &str,
    max_answers: Option<u64>,
) -> Result<serde_json::Value, String> {
    let err = |msg: String| format!("case {case_id}: query failed: {msg}");

    let store = WorldStore::new();
    store.load_nquads(world_nquads).map_err(err)?;

    // Auto-detect the single world (the conformance queries target one world).
    let worlds = store.worlds();
    if worlds.len() != 1 {
        return Err(err(format!(
            "world not given and the store has {} named graphs (need exactly 1)",
            worlds.len()
        )));
    }
    let world = worlds.into_iter().next().expect("len == 1");
    let world_nn = oxigraph::model::NamedNode::new(&world)
        .map_err(|e| err(format!("invalid world IRI {world:?}: {e}")))?;

    let program = parse_query_program(query_text).map_err(err)?;
    let max_answers_usize = max_answers.map(|n| n as usize);

    // Probabilistic profile (#506): weighted model counting; each binding carries a
    // `probability`. This is the only path that emits that key.
    if gmeow_logic::profile_gate::is_probabilistic_profile(profile_str) {
        let answer =
            gmeow_logic::probabilistic::evaluate(&store, &world, &program, profile_str, None)
                .map_err(err)?;
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
        max_steps: None,
    };

    // Counterfactual vs plain backward goal. Both carry a preservation
    // claim disclosing what the target evaluated.
    let (bindings_vec, status, preservation): (
        Vec<gmeow_logic::query_ir::Binding>,
        String,
        gmeow_logic::result::PreservationClaim,
    ) = if gmeow_logic::counterfactual::is_counterfactual(&program) {
        let depth = program
            .counterfactual
            .as_ref()
            .and_then(|c| c.depth_budget)
            .unwrap_or(gmeow_logic::counterfactual::DEFAULT_DEPTH_BUDGET);
        let mut cf = gmeow_logic::counterfactual::construct_and_resolve(
            &store,
            &program,
            profile_str,
            &budget,
            depth,
            None,
        )
        .map_err(err)?;
        let status = cf.status_str().to_string();
        let preservation = cf.result.preservation.clone();
        (std::mem::take(&mut cf.bindings), status, preservation)
    } else {
        let foreign = WorldStoreForeign::from_world(&store, &world, profile_str).map_err(err)?;
        let answer = gmeow_logic::dispatch::dispatch_query(
            &foreign,
            &store,
            &world_nn,
            &program,
            profile_str,
            &budget,
        )
        .map_err(err)?;
        let preservation = answer.preservation.clone();
        (
            answer.bindings,
            answer.status.as_str().to_string(),
            preservation,
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
    Ok(serde_json::json!({
        "bindings": bindings,
        "status": status,
        "preservation": serialize::preservation_to_json(&preservation),
    }))
}

/// Whether a quad is an asserted (EDB) input fact rather than a derived one.
/// (Currently unused by the comparison path; retained for parity with the Python
/// derived/input split and potential future verdicts.)
#[allow(dead_code)]
fn is_asserted(quad: &RunnerQuad) -> bool {
    quad.rule_iri == ASSERT_RULE_IRI
}

// NOTE: `run_case` end-to-end execution over the whole corpus is verified by the
// `datatest-stable` harness (`tests/conformance.rs`), which runs AND diffs every
// case in parallel (~3s). A separate serial smoke test here would only duplicate
// that coverage at ~11s of gate time, so it is intentionally omitted (gate-perf).
//
// The diagnostic-gating firewall (#767 Gap 2) IS unit-tested below because its
// negative branches (a supported contract under `expect_unsupported`, an
// un-declared compile error) are not exercisable through the committed corpus —
// every committed `expected/`-bearing case is a supported preset, so a smoke test
// over a tiny synthetic case dir is the only way to pin those refusals.

#[cfg(test)]
mod gating_tests {
    use super::*;

    /// A throwaway case directory under the system temp dir, removed on drop.
    struct TmpCase(std::path::PathBuf);
    impl TmpCase {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir()
                .join("category")
                .join(format!("gmeow-run-gate-{tag}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("mkdir case dir");
            Self(dir)
        }
        fn write(&self, name: &str, body: &str) {
            std::fs::write(self.0.join(name), body).expect("write case file");
        }
    }
    impl Drop for TmpCase {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A `logic:ReasoningContract` authoring the forbidden probabilistic +
    /// stable-model combination (RuleNoProbabilisticStableModel).
    const UNSUPPORTED_TTL: &str = "\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        @prefix ex: <https://example.org/g/> .\n\
        ex:C a logic:ReasoningContract ;\n\
            logic:modelSemantics logic:StableModelSemantics ;\n\
            logic:uncertaintyMeasure logic:ProbabilisticMeasure .\n\
        ex:m a logic:ProbabilityModel .\n";

    /// A clean, supported positive-Horn domain axiom (no contract, no error).
    const SUPPORTED_TTL: &str = "\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        @prefix ex: <https://example.org/g/> .\n\
        ex:Bird logic:subClassOf ex:Animal .\n";

    #[test]
    fn expect_unsupported_with_forbidden_combo_short_circuits_to_empty() {
        let case = TmpCase::new("ok");
        case.write("input.logic.ttl", UNSUPPORTED_TTL);
        case.write(
            "profile.json",
            r#"{"reasoning_contract":{"preset":"StableModelProfile"},"expect_unsupported":true,"mode":"native"}"#,
        );
        let out = run_case(&case.0).expect("expect_unsupported case must pass");
        // The program was never evaluated: no quads, no answers, empty verdicts.
        assert!(out.materialized_nquads.is_empty());
        assert!(out.answers.is_empty());
        assert_eq!(out.verdicts, serde_json::json!({}));
        // The refusal is disclosed as `{unsupported}` (the legalization floor), never a
        // false `{exact}` that would hide it from a consumer reading `preservation`.
        assert_eq!(
            out.preservation,
            serialize::preservation_to_json(&PreservationClaim::unsupported()),
            "a refused expect_unsupported case must disclose {{unsupported}}, not {{exact}}"
        );
    }

    #[test]
    fn expect_unsupported_but_supported_contract_hard_fails() {
        // The case CLAIMS unsupported but the engine accepts the contract: refuse.
        let case = TmpCase::new("claim");
        case.write("input.logic.ttl", SUPPORTED_TTL);
        case.write(
            "profile.json",
            r#"{"expect_unsupported":true,"mode":"native"}"#,
        );
        let err = run_case(&case.0).unwrap_err();
        assert!(err.contains("expect_unsupported"), "{err}");
        assert!(err.contains("no UNSUPPORTED_CONTRACT"), "{err}");
    }

    #[test]
    fn undeclared_compile_error_hard_fails() {
        // A forbidden combo WITHOUT expect_unsupported must surface as a hard
        // failure (the silent-run hole #767 Gap 2 closes), never a silent evaluate.
        let case = TmpCase::new("silent");
        case.write("input.logic.ttl", UNSUPPORTED_TTL);
        case.write(
            "profile.json",
            r#"{"reasoning_contract":{"preset":"StableModelProfile"},"mode":"native"}"#,
        );
        let err = run_case(&case.0).unwrap_err();
        assert!(err.contains("Severity::Error"), "{err}");
        assert!(err.contains("UNSUPPORTED_CONTRACT"), "{err}");
    }

    // ── verdict_mode = consistency (#753) ─────────────────────────────────────

    const CONSISTENCY_PROFILE: &str = r#"{"verdict_mode":"consistency","mode":"native"}"#;
    const W: &str = "https://gmeow.example/dl/world";

    /// A world-scoped N-Quad EDB line in the gmeow ternary RDF shape.
    fn q(s: &str, p: &str, o: &str) -> String {
        format!("<{s}> <{p}> <{o}> <{W}> .\n")
    }

    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";
    const A: &str = "https://gmeow.example/dl/A";
    const B: &str = "https://gmeow.example/dl/B";
    const C: &str = "https://gmeow.example/dl/C";
    const X: &str = "https://gmeow.example/dl/x";

    #[test]
    fn consistency_mode_populated_clash_is_inconsistent() {
        // x:A, A⊑B, A⊑C, B disjointWith C — x is forced into owl:Nothing, so the
        // world is INCONSISTENT (the external Theorem/Unsatisfiable branch). This
        // exercises the genuine native DL chase (no fake golden).
        let case = TmpCase::new("incon");
        case.write("profile.json", CONSISTENCY_PROFILE);
        let mut nq = String::new();
        nq.push_str(&q(X, RDF_TYPE, A));
        nq.push_str(&q(A, SUBCLASS, B));
        nq.push_str(&q(A, SUBCLASS, C));
        nq.push_str(&q(B, DISJOINT, C));
        case.write("input.nq", &nq);

        let out = run_case(&case.0).expect("consistency case runs");
        assert_eq!(
            out.verdicts[W]["status"], "inconsistent",
            "populated clash must be inconsistent: {}",
            out.verdicts
        );
    }

    #[test]
    fn consistency_mode_clash_free_is_consistent() {
        // x:A, A⊑B with no disjointness — no clash, so the world is CONSISTENT
        // (the external Satisfiable/CounterSatisfiable branch).
        let case = TmpCase::new("con");
        case.write("profile.json", CONSISTENCY_PROFILE);
        let mut nq = String::new();
        nq.push_str(&q(X, RDF_TYPE, A));
        nq.push_str(&q(A, SUBCLASS, B));
        case.write("input.nq", &nq);

        let out = run_case(&case.0).expect("consistency case runs");
        assert_eq!(
            out.verdicts[W]["status"], "consistent",
            "clash-free world must be consistent: {}",
            out.verdicts
        );
    }

    #[test]
    fn consistency_mode_requires_input_nq() {
        // No input.nq ⇒ hard fail (no silent skip / empty verdict).
        let case = TmpCase::new("noedb");
        case.write("profile.json", CONSISTENCY_PROFILE);
        let err = run_case(&case.0).unwrap_err();
        assert!(err.contains("requires input.nq"), "{err}");
    }
}
