// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-case orchestration (`run_case`).
//!
//! Drives the `gmeow_logic` native cores for one conformance case and assembles a
//! typed [`CaseOutputs`], mirroring the retired Python `logic_runner.run` 1:1 by
//! calling the SAME native functions the PyO3 surface wraps (compile → certify →
//! materialize+explain / foundation → answers). There is no PyO3, no Python, and
//! no second engine in this path — the harness is a second *caller* of the engine
//! cores, so its artifacts are identical by construction.
//!
//! Witnesses (`witnesses.json`) are intentionally NOT produced: the runner-contract
//! diff (`logic_runner.diff_case`) never compared them — they are a bless-only side
//! file — so omitting them changes no gate verdict (faithful parity).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use gmeow_logic::compile::frontend::parse_logic_str;
use gmeow_logic::compile::projections::compile_program;
use gmeow_logic::explain::{explain_all, Row};
use gmeow_logic::foundation::{evaluate as foundation_evaluate, AntiRigidityPolicy};
use gmeow_logic::materialize::materialize_routed;
use gmeow_logic::query_ir::{parse_query_program, Budget};
use gmeow_logic::seam::{BudgetStatus, WorldStoreForeign};
use gmeow_logic::store::WorldStore;

use crate::profile::{BudgetParams, Profile};
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
    /// `{query_stem: {"bindings": [...], "status": "..."}}` for each `queries/*.logic`.
    pub answers: BTreeMap<String, serde_json::Value>,
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

    // ── Compile (frontend → IR → projections + nemo rules + ledger) ──────────
    let source = std::fs::read_to_string(case_dir.join("input.logic.ttl"))
        .map_err(|e| prefix(format!("cannot read input.logic.ttl: {e}")))?;
    let (program, _diagnostics) = parse_logic_str(&source, None)
        .map_err(|e| prefix(format!("compile parse failed: {}", e.0)))?;
    let arts = compile_program(&program).map_err(|e| prefix(format!("compile failed: {e}")))?;
    let nemo_rules = arts.nemo_rules.clone();

    // ── Static certification against the declared profile ────────────────────
    let verdict = gmeow_logic::certify::certify(&nemo_rules, &profile.semantic_profile)
        .map_err(|e| prefix(format!("certify failed: {e}")))?;
    let certification = serialize::certification_to_json(&verdict);

    // ── Materialization (+ explanations) ─────────────────────────────────────
    let input_nq = read_optional(case_dir, "input.nq")?;
    let (quads, budget_status, incomplete) = if profile.foundation_lowering {
        materialize_foundation(&case_id, &input_nq, &profile)?
    } else {
        materialize_default(&case_id, &nemo_rules, &input_nq, &profile)?
    };
    let explanations = run_explanations(&case_id, &quads)?;

    // ── N-Quads serialization + downstream artifacts ─────────────────────────
    let materialized_nquads = serialize::materialized_to_nquads(&quads);
    let verdicts = serialize::build_verdicts(&quads);

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

    let projections = ProjectionOutputs {
        rdf,
        report_turtle: arts.report.clone(),
        ledger: serialize::ledger_to_json(&arts.preservation_ledger),
        text,
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
    })
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

/// Strip one outer layer of N3 angle brackets (mirror of `logic_runner._bare_iri`).
fn bare_iri(term: &str) -> String {
    let b = term.as_bytes();
    if b.len() >= 2 && b[0] == b'<' && b[b.len() - 1] == b'>' {
        term[1..term.len() - 1].to_string()
    } else {
        term.to_string()
    }
}

/// Default (non-foundation) materialization: the profile-routed chase. Returns the
/// quads plus the aggregate budget status / incomplete flag (mirrors
/// `logic_runner._materialization_result_from_quad_rows`).
fn materialize_default(
    case_id: &str,
    nemo_rules: &str,
    input_nq: &str,
    profile: &Profile,
) -> Result<(Vec<RunnerQuad>, String, bool), String> {
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

    let exhausted = derived
        .iter()
        .any(|q| q.budget_status == BudgetStatus::Exhausted);
    let quads = derived
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
    Ok((quads, status.to_string(), exhausted))
}

/// Foundation-lowering materialization via the native OntoUML evaluator (mirrors
/// `logic_runner._materialize_foundation`). The foundation evaluator has no budget
/// governor, so a declared `budget_params` is a hard failure.
fn materialize_foundation(
    case_id: &str,
    input_nq: &str,
    profile: &Profile,
) -> Result<(Vec<RunnerQuad>, String, bool), String> {
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
    Ok((quads, "ok".to_string(), false))
}

/// Produce one explanation skeleton per quad (mirrors `logic_runner._run_explanations`
/// + `_explanations_from_rows`). Asserted quads get a trivial depth-0 explanation.
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

/// Resolve every `queries/*.logic` backward goal over the materialized EDB
/// (mirrors `logic_runner._resolve_answers`). Empty map when there is no
/// `queries/` directory.
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
        let answer = gmeow_logic::probabilistic::evaluate(&store, &world, &program, profile_str)
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
            "status": answer.status.as_str(),
        }));
    }

    let budget = Budget {
        max_answers: max_answers_usize,
        max_steps: None,
    };

    // Counterfactual (#505) vs plain backward goal (#504).
    let (bindings_vec, status): (Vec<gmeow_logic::query_ir::Binding>, String) =
        if gmeow_logic::counterfactual::is_counterfactual(&program) {
            let depth = program
                .counterfactual
                .as_ref()
                .and_then(|c| c.depth_budget)
                .unwrap_or(gmeow_logic::counterfactual::DEFAULT_DEPTH_BUDGET);
            let cf = gmeow_logic::counterfactual::construct_and_resolve(
                &store,
                &program,
                profile_str,
                &budget,
                depth,
            )
            .map_err(err)?;
            (cf.bindings, cf.status.as_str().to_string())
        } else {
            let foreign =
                WorldStoreForeign::from_world(&store, &world, profile_str).map_err(err)?;
            let answer = gmeow_logic::dispatch::dispatch_query(
                &foreign,
                &store,
                &world_nn,
                &program,
                profile_str,
                &budget,
            )
            .map_err(err)?;
            (answer.bindings, answer.status.as_str().to_string())
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
    Ok(serde_json::json!({ "bindings": bindings, "status": status }))
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
