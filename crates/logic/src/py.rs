// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 Python bindings for `gmeow-logic`.
//!
//! # Nemo wire-up (issue #501 Task 4)
//!
//! `materialize` drives the full Nemo chase WITH real proof-trace provenance. The
//! engine pipeline itself — parse N-Quads → encode Nemo facts → run the chase →
//! decode rows to `DerivedQuad`s with real provenance → apply the post-hoc budget
//! governor — lives in [`crate::materialize`] (pure Rust, natively `#[test]`ed).
//! This module keeps only the PyO3 marshalling shell:
//!
//! 1. Short-circuit empty input to an empty result.
//! 2. Route non-stratifiable rule sets to the native evaluators (issue #651).
//! 3. Delegate to [`crate::materialize::materialize_core`] off the GIL.
//! 4. Serialize the resulting `DerivedQuad`s to Python dicts.
//!
//! Encode/decode helpers (oxigraph term ⇄ Nemo fact string) live in
//! [`crate::encode`]; this module handles the PyO3 surface.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use oxigraph::io::RdfFormat;
use oxigraph::model::NamedNode;
use oxigraph::store::Store;

use crate::certify::certify as certify_rules;
use crate::dispatch::dispatch_query;
use crate::materialize::{materialize_core, MaterializeError, ASSERTED_PROFILE};
use crate::query_ir::{parse_query_program, Budget};
use crate::rule_ir::{DerivedRow, EvalRule};
use crate::seam::{DerivedQuad, WorldStoreForeign};
use crate::store::WorldStore;

// ── Non-stratifiable native routing (issue #651) ────────────────────────────────
//
// The Nemo chase rejects negation-in-a-cycle outright (`SelectionStrategyError`),
// so it cannot evaluate the well-founded or stable-model semantics. Those are
// evaluated by native Rust ([`crate::wellfounded`] / [`crate::stablemodel`]) —
// no Nemo, no Python oracle. A declared `StratifiedNAFProfile` set that fails
// stratification (the certifier's negative control) cannot run on Nemo either; it
// materialises asserted-only (the lossy-positive minimal), with the projection
// loss recorded on the Python side. Everything else (PositiveHorn, genuinely
// stratified NAF, projection-only EDB round-trips) falls through to the Nemo path
// unchanged — `profile = None` preserves the pre-#651 behaviour byte-for-byte.

/// Load an N-Quads string into a world-indexed [`WorldStore`].
fn world_store_from_nquads(input: &str) -> PyResult<WorldStore> {
    let store = WorldStore::new();
    store
        .load_nquads(input)
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    Ok(store)
}

/// Parse `.rls` text into the native evaluable rule IR.
fn parse_eval_rules(rules: &str) -> PyResult<Vec<EvalRule>> {
    crate::rule_ir::parse_eval_rules(rules).map_err(pyo3::exceptions::PyValueError::new_err)
}

/// Serialise native [`DerivedRow`]s to the SAME Python-dict shape as
/// [`derived_quad_to_dict`] (the materialize seam contract), so the runner's
/// row→`MaterializationResult` adapter consumes them unchanged. The native
/// non-stratifiable paths run to a polynomial fixpoint / bounded enumeration with
/// no budget ceiling, so `budget_status` is always `"ok"`.
fn derived_rows_to_dicts(py: Python<'_>, rows: &[DerivedRow]) -> PyResult<Vec<Py<PyAny>>> {
    rows.iter()
        .map(|r| {
            let d = PyDict::new(py);
            d.set_item("graph", r.graph.as_str())?;
            d.set_item("subject", r.subject.to_string())?;
            d.set_item("predicate", r.predicate.as_str())?;
            d.set_item("object", r.object.to_string())?;
            d.set_item("graph_component", r.graph.as_str())?;
            d.set_item("derivation_id", r.derivation_id.as_str())?;
            d.set_item("rule_iri", r.rule_iri.as_str())?;
            d.set_item("source_quad_ids", r.source_quad_ids.clone())?;
            d.set_item("profile", ASSERTED_PROFILE)?;
            d.set_item("budget_status", "ok")?;
            Ok(d.into_any().unbind())
        })
        .collect()
}

/// Echo only the asserted EDB facts (per world) as materialized quads. Used for a
/// declared `StratifiedNAFProfile` set that is genuinely non-stratifiable: Nemo
/// would reject it, the well-founded/stable evaluators are not selected, so the
/// honest minimal materialization is the input itself (loss recorded Python-side).
fn echo_edb_only(input: &str) -> PyResult<Vec<DerivedRow>> {
    let store = world_store_from_nquads(input)?;
    let mut worlds = store.worlds();
    worlds.sort();
    let mut rows: Vec<DerivedRow> = Vec::new();
    for world in &worlds {
        let edb = crate::rule_ir::world_edb_facts(&store, world)
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
        rows.extend(
            crate::rule_ir::echo_asserted(world, &edb)
                .map_err(pyo3::exceptions::PyRuntimeError::new_err)?,
        );
    }
    Ok(rows)
}

/// Route a non-stratifiable program to its native evaluator, returning `Some(rows)`
/// when handled, or `None` to fall through to the Nemo chase.
fn route_non_stratifiable(
    py: Python<'_>,
    rules: &str,
    input: &str,
    profile: Option<&str>,
) -> PyResult<Option<Vec<Py<PyAny>>>> {
    let rows: Vec<DerivedRow> = match profile {
        Some("WellFoundedProfile") => {
            let store = world_store_from_nquads(input)?;
            let eval_rules = parse_eval_rules(rules)?;
            crate::wellfounded::materialize(&store, &eval_rules).map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "well-founded materialization failed: {e}"
                ))
            })?
        }
        Some("StableModelProfile") => {
            let store = world_store_from_nquads(input)?;
            let eval_rules = parse_eval_rules(rules)?;
            crate::stablemodel::cautious_materialize(&store, &eval_rules).map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "stable-model materialization failed: {e}"
                ))
            })?
        }
        _ => {
            // PositiveHorn / declared StratifiedNAF / None / projection-only.
            // Empty rules (projection-only) and genuinely stratified sets run on
            // Nemo; only a declared set that FAILS stratification is echoed here.
            if rules.trim().is_empty() {
                return Ok(None);
            }
            let stratifiable = crate::certify::is_stratifiable(rules)
                .map_err(pyo3::exceptions::PyValueError::new_err)?;
            if stratifiable {
                return Ok(None);
            }
            echo_edb_only(input)?
        }
    };
    Ok(Some(derived_rows_to_dicts(py, &rows)?))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Convert a [`DerivedQuad`] to a Python dict with all metadata fields.
///
/// Keys exposed to Python:
/// - `graph`           — named-graph IRI string (the world)
/// - `subject`         — S IRI/term string
/// - `predicate`       — P IRI string
/// - `object`          — O IRI/term string
/// - `graph_component` — same as `graph` (quad self-contained, per seam contract)
/// - `derivation_id`   — IRI string
/// - `rule_iri`        — IRI string
/// - `source_quad_ids` — list of IRI strings
/// - `profile`         — IRI string
/// - `budget_status`   — canonical lowercase string (`"ok"`, `"partial"`, `"exhausted"`)
fn derived_quad_to_dict(py: Python<'_>, dq: &DerivedQuad) -> PyResult<Py<PyAny>> {
    let d = PyDict::new(py);
    d.set_item("graph", dq.graph.as_str())?;
    d.set_item("subject", dq.subject.to_string())?;
    d.set_item("predicate", dq.predicate.as_str())?;
    d.set_item("object", dq.object.to_string())?;
    d.set_item("graph_component", dq.graph_component.as_str())?;
    d.set_item("derivation_id", dq.derivation_id.as_str())?;
    d.set_item("rule_iri", &dq.rule_iri)?;
    d.set_item("source_quad_ids", &dq.source_quad_ids)?;
    d.set_item("profile", &dq.profile)?;
    d.set_item("budget_status", dq.budget_status.as_str())?;
    Ok(d.into_any().unbind())
}

// ── materialize ───────────────────────────────────────────────────────────────

/// Run the Nemo chase against `input` (N-Quads) and `rules` (`.rls` text).
///
/// # Arguments
///
/// - `rules` — Nemo rule-language string (may be empty for a pure EDB round-trip).
/// - `input` — N-Quads string.  Each quad is encoded as a Nemo ground fact and
///             fed as EDB to the chase.  The named-graph IRI is the "world".
///
/// # Returns
///
/// A list of Python dicts, one per derived quad (including EDB facts, since
/// Nemo returns EDB predicates in `derived_predicates()`).  Each dict carries
/// the full seam metadata: graph, subject, predicate, object, graph_component,
/// derivation_id, rule_iri, source_quad_ids, profile, budget_status.
///
/// Provenance is real — not stubs:
/// - Asserted (EDB) quads carry `rule_iri = logic:assert`,
///   `source_quad_ids = [self_reifier]`, and a content-addressed `derivation_id`.
/// - Derived (IDB) quads carry the firing rule's IRI (from `#[name("...")]`),
///   `source_quad_ids` of the immediate antecedents, and a content-addressed
///   `derivation_id`.
///
/// An empty (or whitespace-only) `input` returns an empty list immediately
/// without invoking the chase.
///
/// # Budget governor (issue #502)
///
/// The optional `max_rule_firings`, `max_answers`, and `time_ms` parameters bound
/// the run. Asserted **EDB input facts are always kept in full** — a budget never
/// drops a given input quad; only **derived (IDB)** quads are bounded.
///
/// **The count ceilings (`max_rule_firings`, `max_answers`) are engine-independent
/// and deterministic.** Nemo's `reason()` runs to full fixpoint with no native
/// budget hook, and the Python oracle likewise runs to fixpoint under these
/// ceilings; both then truncate the derived set *post-hoc* to the canonical-sort
/// prefix of the **complete** derivation and stamp the kept quads
/// `BudgetStatus::Exhausted`. Because both engines compute the same fixpoint and
/// keep the same canonical prefix, the count ceilings yield identical verdicts.
///
/// **Only `time_ms` is engine-dependent.** The Python oracle can cut the chase
/// mid-flight on the wall clock; on the Rust side `time_ms` bounds only the
/// *post-fixpoint* work (decode + bookkeeping), not the chase itself. A genuinely
/// non-terminating rule set is the static certifier's job to reject up front (see
/// [`crate::certify`]), not the governor's to interrupt.
///
/// The `time_ms` divergence is **named, not glossed** (honesty invariant): under
/// the count ceilings the verdict and budget strings match the oracle exactly;
/// only on a wall-clock cut do the behaviours legitimately differ, and that
/// difference is documented here, in `certify.rs`, and in `crates/logic/README.md`.
///
/// When a ceiling trips, kept rows are a **sound subset** of the full fixpoint —
/// a prefix of the canonical (graph, S, P, O) sort — never fabricated. With all
/// three parameters `None` (the default), the output is **byte-identical to
/// pre-#502**: every quad keeps `budget_status = "ok"` and the chase-order output
/// is preserved unchanged.
///
/// # Errors
///
/// Returns a Python `ValueError` for N-Quads parse errors and
/// `RuntimeError` for chase or decode failures.
#[pyfunction]
#[pyo3(signature = (rules, input, max_rule_firings=None, max_answers=None, time_ms=None, profile=None))]
fn materialize(
    py: Python<'_>,
    rules: &str,
    input: &str,
    max_rule_firings: Option<u64>,
    max_answers: Option<u64>,
    time_ms: Option<u64>,
    profile: Option<&str>,
) -> PyResult<Vec<Py<PyAny>>> {
    // ── Short-circuit: nothing to do ──────────────────────────────────────────
    if input.trim().is_empty() {
        return Ok(vec![]);
    }

    // ── Non-stratifiable native routing (issue #651) ─────────────────────────
    // Well-founded / stable-model semantics (and a declared-StratifiedNAF set that
    // fails stratification) are evaluated natively — the Nemo chase below rejects
    // negation-in-a-cycle. `profile = None` with stratifiable rules returns `None`
    // here and falls through to the byte-identical pre-#651 Nemo path.
    if let Some(rows) = route_non_stratifiable(py, rules, input, profile)? {
        return Ok(rows);
    }

    // ── Pure engine pipeline (GIL released) ──────────────────────────────────
    // The whole parse → encode → chase → decode → budget pipeline is engine work
    // with no Python contact, so we run it off the GIL. See [`crate::materialize`]
    // for the implementation and its native `#[test]` coverage; the FFI keeps only
    // the marshalling shell below.
    let final_quads = py
        .detach(|| materialize_core(rules, input, max_rule_firings, max_answers, time_ms))
        .map_err(|e| match e {
            MaterializeError::Parse(m) => pyo3::exceptions::PyValueError::new_err(m),
            MaterializeError::Chase(m) => pyo3::exceptions::PyRuntimeError::new_err(m),
        })?;

    // ── Serialize to Python dicts ────────────────────────────────────────────
    final_quads
        .iter()
        .map(|dq| derived_quad_to_dict(py, dq))
        .collect()
}

// ── certify ─────────────────────────────────────────────────────────────────

/// Statically certify a Nemo `.rls` rule set against a declared semantic profile.
///
/// This is the Rust mirror of the Python oracle
/// (`gmeow_tools.logic_certify.certify_program`). The returned dict has the SAME
/// shape, keys, and values as Python `CertificationVerdict.to_json()`:
///
/// ```python
/// {
///   "certified": bool,
///   "decidability_class": str,
///   "profile_id": str,
///   "violations": [str, …]   # sorted, byte-identical to the oracle
/// }
/// ```
///
/// `profile` matches the Python profile-id strings, e.g. `"PositiveHornProfile"`,
/// `"StratifiedNAFProfile"`, `"StableModelProfile"`. Certification uses
/// *sufficient* conditions and is *necessarily incomplete* (termination is
/// undecidable): a clean verdict proves membership in the declared
/// decidable/terminating fragment; a violation only proves the cheap structural
/// condition does not hold.
///
/// # Errors
///
/// Returns a Python `ValueError` if `rules` is not parseable Nemo `.rls`.
#[pyfunction]
fn certify(py: Python<'_>, rules: &str, profile: &str) -> PyResult<Py<PyAny>> {
    let verdict = certify_rules(rules, profile).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("certify parse error: {e}"))
    })?;
    let (certified, decidability_class, profile_id, violations) = verdict.to_json_pairs();

    let d = PyDict::new(py);
    // Insert in the same sorted-key order Python's `to_json()` literal uses.
    d.set_item("certified", certified)?;
    d.set_item("decidability_class", decidability_class)?;
    d.set_item("profile_id", profile_id)?;
    d.set_item("violations", violations)?;
    Ok(d.into_any().unbind())
}

// ── query ───────────────────────────────────────────────────────────────────

/// Resolve a `.logic` backward goal over a materialized world (issue #504, v4).
///
/// This is the AC-1/AC-2 engine surface: it loads the materialized EDB, parses the
/// `.logic` query program (rules + goal), enforces the cut/profile gate, and routes
/// the goal through the dispatcher (oxigraph SPARQL fast path for non-recursive
/// pattern goals; embedded Scryer Prolog with tabling for recursive/unification-heavy
/// goals). Answers are **virtual** — nothing is written back into the store.
///
/// # Arguments
///
/// - `world_nquads` — the materialized world(s) as N-Quads (named graphs = worlds).
/// - `query_program` — the `.logic` source (prefixes, Horn rules, optional `!` cut,
///   exactly one `?- goal.`).
/// - `profile` — the semantic profile in force (bare name or IRI). Cut is permitted
///   ONLY under `ProceduralPrologProfile`; otherwise this raises `ValueError`. Under
///   `ProbabilisticProfile` (#506) the goal is resolved by weighted model counting and
///   each binding carries a `probability` (see [`crate::probabilistic`]).
/// - `world_iri` — which world to resolve against. If `None`, the store must contain
///   exactly one named graph (auto-selected); otherwise this is an error.
/// - `max_answers` — output cap (→ `status="partial"`).
/// - `max_steps` — inference-count ceiling (→ `status="exhausted"`).
///
/// # Returns
///
/// A dict `{"bindings": [ {var: canonical_str, …}, … ], "status": "ok"|"partial"|"exhausted"}`
/// where each canonical value is the oracle/engine `Const` form (`<iri>` for IRIs).
/// The binding list is canonically sorted for determinism.
///
/// # Errors
///
/// Raises Python `ValueError` on malformed N-Quads/query, a missing/ambiguous world,
/// a cut outside `ProceduralPrologProfile`, or a Scryer/resolution error.
#[pyfunction]
#[pyo3(signature = (world_nquads, query_program, profile, world_iri=None, max_answers=None, max_steps=None))]
fn query(
    py: Python<'_>,
    world_nquads: &str,
    query_program: &str,
    profile: &str,
    world_iri: Option<&str>,
    max_answers: Option<usize>,
    max_steps: Option<u64>,
) -> PyResult<Py<PyAny>> {
    let value_err = |msg: String| pyo3::exceptions::PyValueError::new_err(msg);

    // 1. Load the materialized EDB into a world-indexed store.
    let store = WorldStore::new();
    store.load_nquads(world_nquads).map_err(value_err)?;

    // 2. Resolve the target world (explicit, or the single named graph).
    let world = match world_iri {
        Some(w) => w.to_owned(),
        None => {
            let worlds = store.worlds();
            if worlds.len() != 1 {
                return Err(value_err(format!(
                    "world_iri not given and the store has {} named graphs \
                     (need exactly 1): {worlds:?}",
                    worlds.len()
                )));
            }
            worlds.into_iter().next().expect("len == 1")
        }
    };
    let world_nn = NamedNode::new(&world)
        .map_err(|e| value_err(format!("invalid world IRI {world:?}: {e}")))?;

    // 3. Parse the query program (rules + goal).
    let program = parse_query_program(query_program).map_err(value_err)?;

    // 3b. Probabilistic profile (#506, v6): marginal inference by weighted model
    //     counting routes here instead of the backward-goal dispatcher. Each binding
    //     carries a `probability`; this is the ONLY path that emits that key, so
    //     non-probabilistic answers stay byte-identical. confidence/weight/evidence
    //     never enter the marginal — the confidence≠probability guard is structural.
    if crate::profile_gate::is_probabilistic_profile(profile) {
        let answer =
            crate::probabilistic::evaluate(&store, &world, &program, profile).map_err(value_err)?;
        let bindings = PyList::empty(py);
        for binding in &answer.bindings {
            let row = PyDict::new(py);
            for (var, val) in &binding.vars {
                row.set_item(var, val)?;
            }
            row.set_item("probability", binding.probability)?;
            bindings.append(row)?;
        }
        let result = PyDict::new(py);
        result.set_item("bindings", bindings)?;
        result.set_item("status", answer.status.as_str())?;
        return Ok(result.into_any().unbind());
    }

    // 4. Build the read-only EDB accessor for this world.
    let foreign = WorldStoreForeign::from_world(&store, &world, profile).map_err(value_err)?;

    // 5. Dispatch. A Stratum-C counterfactual program (#505) routes through
    //    transient world construction; a plain v4 backward goal runs against the
    //    materialized world via the cut/profile-gated dispatcher.
    let budget = Budget {
        max_answers,
        max_steps,
    };
    // The counterfactual path returns a CfAnswer (status may be "unknown" or
    // "incomplete"); the plain path returns an AnswerSet. Normalize both to a
    // binding list plus a canonical status string.
    let (answer_bindings, status_str): (Vec<crate::query_ir::Binding>, String) =
        if crate::counterfactual::is_counterfactual(&program) {
            // Honor a program-declared `depth_budget(N)`; otherwise the engine default.
            let depth = program
                .counterfactual
                .as_ref()
                .and_then(|c| c.depth_budget)
                .unwrap_or(crate::counterfactual::DEFAULT_DEPTH_BUDGET);
            let cf = crate::counterfactual::construct_and_resolve(
                &store, &program, profile, &budget, depth,
            )
            .map_err(value_err)?;
            (cf.bindings, cf.status.as_str().to_owned())
        } else {
            let answer = dispatch_query(&foreign, &store, &world_nn, &program, profile, &budget)
                .map_err(value_err)?;
            (answer.bindings, answer.status.as_str().to_owned())
        };

    // 6. Build the Python result dict: {"bindings": [...], "status": "..."}.
    let bindings = PyList::empty(py);
    for binding in &answer_bindings {
        let row = PyDict::new(py);
        for (var, val) in binding {
            row.set_item(var, val)?;
        }
        bindings.append(row)?;
    }
    let result = PyDict::new(py);
    result.set_item("bindings", bindings)?;
    result.set_item("status", status_str)?;
    Ok(result.into_any().unbind())
}

// ── foundation ──────────────────────────────────────────────────────────────────

/// Evaluate the OntoUML *foundation* disciplines natively (issue #636).
///
/// Native Rust port of the Python foundation oracle
/// (`gmeow_tools.logic_foundation` + the `enable_naf` materializer path).  Parses
/// `input` N-Quads into a world-indexed [`WorldStore`] (named graphs = worlds),
/// runs the stratified semi-naive chase plus the cross-world rigidity and
/// anti-rigidity post-passes, and returns the asserted + derived quads as Python
/// dicts.  Provenance (reifier + derivation IDs) is byte-identical to the oracle
/// (see [`crate::foundation`]).
///
/// # Arguments
///
/// - `input` — the EDB facts as N-Quads (named graphs = worlds).
/// - `anti_rigidity_policy` — one of `"witness-obligation"` (default),
///   `"schema-only"`, `"witness-required"`.  An unknown value is a HARD FAILURE
///   (raises `ValueError`) — the policy is a closed enum with no silent default.
///
/// # Returns
///
/// A `list[dict]` with keys `graph`, `subject`, `predicate`, `obj`,
/// `derivation_id`, `rule_iri`, `source_quad_ids`, `profile`, `budget_status`.
///
/// # Errors
///
/// Raises `ValueError` for an N-Quads parse error or an unknown policy, and
/// `RuntimeError` for an internal evaluation/provenance failure.
#[pyfunction]
#[pyo3(signature = (input, anti_rigidity_policy=None))]
fn foundation(
    py: Python<'_>,
    input: &str,
    anti_rigidity_policy: Option<&str>,
) -> PyResult<Vec<Py<PyAny>>> {
    use crate::foundation::{evaluate, AntiRigidityPolicy};

    // Closed enum — unknown value is a hard error (no silent default).  The default
    // when the key is absent is "witness-obligation".
    let policy = match anti_rigidity_policy {
        Some(value) => {
            AntiRigidityPolicy::from_str(value).map_err(pyo3::exceptions::PyValueError::new_err)?
        }
        None => AntiRigidityPolicy::WitnessObligation,
    };

    if input.trim().is_empty() {
        return Ok(vec![]);
    }

    // Parse N-Quads into a world-indexed store (preserving worlds = named graphs).
    let store = WorldStore::new();
    store
        .load_nquads(input)
        .map_err(pyo3::exceptions::PyValueError::new_err)?;

    let quads = evaluate(&store, policy).map_err(|e| {
        pyo3::exceptions::PyRuntimeError::new_err(format!("foundation evaluation failed: {e}"))
    })?;

    let profile = crate::foundation::profile_iri();
    let budget_status = crate::foundation::budget_status();

    let mut out: Vec<Py<PyAny>> = Vec::with_capacity(quads.len());
    for q in &quads {
        let d = PyDict::new(py);
        d.set_item("graph", q.graph.as_str())?;
        d.set_item("subject", q.subject.as_str())?;
        d.set_item("predicate", q.predicate.as_str())?;
        d.set_item("obj", q.object.as_str())?;
        d.set_item("derivation_id", q.derivation_id.as_str())?;
        d.set_item("rule_iri", q.rule_iri.as_str())?;
        d.set_item("source_quad_ids", q.source_quad_ids.clone())?;
        d.set_item("profile", profile)?;
        d.set_item("budget_status", budget_status)?;
        out.push(d.into_any().unbind());
    }
    Ok(out)
}

/// `explain(quads) -> list[dict]` — native explanation skeletons (issue #497).
///
/// Reconstructs the derivation tree for every quad in `quads` (one explanation per
/// input quad, IN INPUT ORDER) and returns the cited-IRI skeleton — the conformance
/// surface.  This is the byte-faithful Rust port of the retired Python explanation
/// oracle (`gmeow_tools.logic_explain`); prose rendering is intentionally not
/// reproduced (the runner compares only `cited_iris` and matches by
/// `target_quad_reifier`).
///
/// # Arguments
///
/// - `quads` — a `list[dict]`, each with keys `graph`, `subject`, `predicate`,
///   `obj` (object in canonical N3 form), `derivation_id`, `rule_iri`, and
///   `source_quad_ids` (a `list[str]` of antecedent reifier IRIs).
///
/// # Returns
///
/// A `list[dict]` (one per input quad, same order) with keys:
/// - `target_quad_reifier` (str)
/// - `world_iri` (str)
/// - `target_derivation_id` (str)
/// - `cited_iris` (sorted `list[str]`)
/// - `step_skeleton` (`list[dict]`, each carrying the full [`crate::explain::ExplanationStep`]
///   surface: `derivation_id`, `rule_iri`, `quad_reifier`, `subject_iri`, `predicate_iri`,
///   `obj_n3`, `graph_iri`, `term_iris`, `source_step_ids`, `is_asserted`, `depth`).
///
/// # Errors
///
/// Raises `ValueError` for a malformed input row (missing/ill-typed key) and
/// `RuntimeError` for a reconstruction failure (unresolved antecedent, cycle).
#[pyfunction]
fn explain(py: Python<'_>, quads: Vec<Bound<'_, PyDict>>) -> PyResult<Vec<Py<PyAny>>> {
    let rows = explain_rows_from_dicts(&quads)?;
    explain_rows_to_dicts(py, &rows)
}

/// Decode a list of materialize/explain quad dicts into [`crate::explain::Row`]s.
///
/// This is the SINGLE decode path used by both the standalone `explain` pyfunction
/// and the fused `materialize_explained` (issue #630): there is no second,
/// divergent decoder.  Each dict carries `graph`, `subject`, `predicate`, `obj`
/// (or `object`), `derivation_id`, `rule_iri`, and `source_quad_ids`.
///
/// The `subject` is normalized to a BARE IRI: the materialize seam emits the
/// subject in N3 display form (`<iri>`), while the explanation reifier recipe wraps
/// the subject in `<...>` itself, so a doubly-wrapped value would mint the wrong
/// reifier.  Stripping one outer `<...>` layer mirrors the Python runner's
/// `_bare_iri` helper exactly; a blank-node / already-bare value passes through.
fn explain_rows_from_dicts(quads: &[Bound<'_, PyDict>]) -> PyResult<Vec<crate::explain::Row>> {
    use crate::explain::Row;

    fn get_str(d: &Bound<'_, PyDict>, key: &str) -> PyResult<String> {
        let item = d.get_item(key)?.ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("explain: row missing key {key:?}"))
        })?;
        item.extract::<String>().map_err(|_| {
            pyo3::exceptions::PyValueError::new_err(format!("explain: key {key:?} must be a str"))
        })
    }

    /// Try `"obj"` first (the explain payload convention), then fall back to
    /// `"object"` (the key emitted by `derived_quad_to_dict` / materialize).
    /// Raises `PyValueError` if neither key is present.
    fn get_obj_str(d: &Bound<'_, PyDict>) -> PyResult<String> {
        if let Some(item) = d.get_item("obj")? {
            return item.extract::<String>().map_err(|_| {
                pyo3::exceptions::PyValueError::new_err("explain: key \"obj\" must be a str")
            });
        }
        if let Some(item) = d.get_item("object")? {
            return item.extract::<String>().map_err(|_| {
                pyo3::exceptions::PyValueError::new_err("explain: key \"object\" must be a str")
            });
        }
        Err(pyo3::exceptions::PyValueError::new_err(
            "explain: row missing key \"obj\" (or \"object\")",
        ))
    }

    let mut rows: Vec<Row> = Vec::with_capacity(quads.len());
    for d in quads {
        let sources_item = d.get_item("source_quad_ids")?.ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err("explain: row missing key \"source_quad_ids\"")
        })?;
        let source_quad_ids: Vec<String> = sources_item.extract().map_err(|_| {
            pyo3::exceptions::PyValueError::new_err(
                "explain: key \"source_quad_ids\" must be a list[str]",
            )
        })?;
        rows.push(Row {
            graph: get_str(d, "graph")?,
            subject: bare_iri(&get_str(d, "subject")?),
            predicate: get_str(d, "predicate")?,
            obj: get_obj_str(d)?,
            derivation_id: get_str(d, "derivation_id")?,
            rule_iri: get_str(d, "rule_iri")?,
            source_quad_ids,
        });
    }
    Ok(rows)
}

/// Strip one outer layer of N3 angle brackets from a term surface (the Rust mirror
/// of the Python runner's `_bare_iri`).  A blank-node (`_:b`) or already-bare value
/// passes through unchanged.
fn bare_iri(term_surface: &str) -> String {
    let bytes = term_surface.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'<' && bytes[bytes.len() - 1] == b'>' {
        term_surface[1..term_surface.len() - 1].to_owned()
    } else {
        term_surface.to_owned()
    }
}

/// Run [`crate::explain::explain_all`] over `rows` and serialize each
/// [`crate::explain::Explanation`] to the Python dict shape the runner consumes.
///
/// This is the SINGLE serialization path used by both `explain` and
/// `materialize_explained` (issue #630).
fn explain_rows_to_dicts(py: Python<'_>, rows: &[crate::explain::Row]) -> PyResult<Vec<Py<PyAny>>> {
    let explanations = crate::explain::explain_all(rows)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("explain failed: {e}")))?;

    let mut out: Vec<Py<PyAny>> = Vec::with_capacity(explanations.len());
    for exp in &explanations {
        let d = PyDict::new(py);
        d.set_item("target_quad_reifier", exp.target_quad_reifier.as_str())?;
        d.set_item("world_iri", exp.world_iri.as_str())?;
        d.set_item("target_derivation_id", exp.target_derivation_id.as_str())?;
        // cited_iris is a BTreeSet — already sorted; emit as a list[str].
        let cited: Vec<&str> = exp.cited_iris.iter().map(String::as_str).collect();
        d.set_item("cited_iris", cited)?;

        let steps = PyList::empty(py);
        for step in &exp.step_skeleton {
            let sd = PyDict::new(py);
            sd.set_item("derivation_id", step.derivation_id.as_str())?;
            sd.set_item("rule_iri", step.rule_iri.as_str())?;
            sd.set_item("quad_reifier", step.quad_reifier.as_str())?;
            sd.set_item("subject_iri", step.subject_iri.as_str())?;
            sd.set_item("predicate_iri", step.predicate_iri.as_str())?;
            sd.set_item("obj_n3", step.obj_n3.as_str())?;
            sd.set_item("graph_iri", step.graph_iri.as_str())?;
            let terms: Vec<&str> = step.term_iris.iter().map(String::as_str).collect();
            sd.set_item("term_iris", terms)?;
            let src_ids: Vec<&str> = step.source_step_ids.iter().map(String::as_str).collect();
            sd.set_item("source_step_ids", src_ids)?;
            sd.set_item("is_asserted", step.is_asserted)?;
            sd.set_item("depth", step.depth)?;
            steps.append(sd)?;
        }
        d.set_item("step_skeleton", steps)?;
        out.push(d.into_any().unbind());
    }
    Ok(out)
}

// ── materialize_explained ─────────────────────────────────────────────────────

/// Fuse `materialize` + `explain` into one native call (issue #630).
///
/// Runs the EXACT same chase as [`materialize`] and then the EXACT same
/// explanation skeleton as [`explain`] over the in-memory derivation — with NO
/// Rust→Python→Rust payload round-trip.  The two halves reuse the shared internals
/// (no forked chase, no forked explain decode/serialize):
///
/// * the chase output is produced by calling [`materialize`] verbatim, so the
///   `quads` key is byte-identical to what `materialize` returns today;
/// * the explanation skeleton is produced by decoding those same quad dicts through
///   [`explain_rows_from_dicts`] and serializing through [`explain_rows_to_dicts`]
///   — the same two helpers the standalone `explain` pyfunction uses — so the
///   `explanations` key is byte-identical to what `explain` returns today.
///
/// # Returns
///
/// A dict with two keys:
/// * `quads` — the `list[dict]` `materialize` returns (keys: graph, subject,
///   predicate, object, graph_component, derivation_id, rule_iri, source_quad_ids,
///   profile, budget_status).
/// * `explanations` — the `list[dict]` `explain` returns (one per quad, in order).
///
/// # Errors
///
/// Propagates any error from the chase ([`materialize`]) or the explanation
/// reconstruction ([`explain_rows_to_dicts`]).
#[pyfunction]
#[pyo3(signature = (rules, input, max_rule_firings=None, max_answers=None, time_ms=None, profile=None))]
fn materialize_explained<'py>(
    py: Python<'py>,
    rules: &str,
    input: &str,
    max_rule_firings: Option<u64>,
    max_answers: Option<u64>,
    time_ms: Option<u64>,
    profile: Option<&str>,
) -> PyResult<Bound<'py, PyDict>> {
    // 1. Run the chase exactly as `materialize` does — same helpers, same output.
    let quad_objs = materialize(
        py,
        rules,
        input,
        max_rule_firings,
        max_answers,
        time_ms,
        profile,
    )?;

    // 2. Decode those same quad dicts into explain Rows (the SAME decoder the
    //    standalone `explain` pyfunction uses) and run the explanation skeleton —
    //    no payload is rebuilt in Python, no FFI boundary is recrossed.
    let quad_dicts: Vec<Bound<'py, PyDict>> = quad_objs
        .iter()
        .map(|obj| obj.bind(py).clone().cast_into::<PyDict>())
        .collect::<Result<_, _>>()
        .map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "materialize_explained: chase row was not a dict: {e}"
            ))
        })?;
    let explanation_rows = explain_rows_from_dicts(&quad_dicts)?;
    let explanation_objs = explain_rows_to_dicts(py, &explanation_rows)?;

    // 3. Assemble the fused result dict.
    let out = PyDict::new(py);
    out.set_item("quads", quad_objs)?;
    out.set_item("explanations", explanation_objs)?;
    Ok(out)
}

/// `stable_models(rules, input) -> dict` — enumerate the stable models (answer
/// sets) of a non-stratifiable program, per world (issue #651).
///
/// The cautious (skeptical) intersection of these models is what
/// `materialize(..., profile="StableModelProfile")` emits as quads; this entry
/// surfaces the individual models for the conformance `witnesses.json` side file,
/// keeping the materialized quad set honest (an even loop's cautious core is
/// empty, so its `materialized.nq` stays asserted-only).
///
/// # Returns
///
/// A dict keyed by world IRI; each value is a `list` of models, each model a
/// `list` of atom dicts `{subject, predicate, object}` (atoms sorted canonically,
/// models in canonical enumeration order). An empty `input` returns `{}`.
///
/// # Errors
///
/// Raises `ValueError` for unparsable `.rls` / N-Quads and `RuntimeError` for an
/// evaluation failure.
#[pyfunction]
#[pyo3(signature = (rules, input))]
fn stable_models(py: Python<'_>, rules: &str, input: &str) -> PyResult<Py<PyAny>> {
    let out = PyDict::new(py);
    if input.trim().is_empty() {
        return Ok(out.into_any().unbind());
    }
    let store = world_store_from_nquads(input)?;
    let eval_rules = parse_eval_rules(rules)?;
    let per_world = crate::stablemodel::stable_models(&store, &eval_rules).map_err(|e| {
        pyo3::exceptions::PyRuntimeError::new_err(format!("stable_models failed: {e}"))
    })?;
    for (world, models) in &per_world {
        let models_list = PyList::empty(py);
        for model in models {
            let atoms_list = PyList::empty(py);
            for fact in &model.atoms {
                let ad = PyDict::new(py);
                ad.set_item("subject", fact.subject.to_string())?;
                ad.set_item("predicate", fact.predicate.as_str())?;
                ad.set_item("object", fact.object.to_string())?;
                atoms_list.append(ad)?;
            }
            models_list.append(atoms_list)?;
        }
        out.set_item(world.as_str(), models_list)?;
    }
    Ok(out.into_any().unbind())
}

// ── Module registration ───────────────────────────────────────────────────────

/// Python extension module `gmeow_logic`.
///
/// Exposes:
/// - `materialize(rules, input, max_rule_firings=None, max_answers=None, time_ms=None, profile=None)`
/// - `materialize_explained(rules, input, …) -> {"quads": [...], "explanations": [...]}`
///   — the fused chase+explain call (issue #630)
/// - `foundation(input, anti_rigidity_policy=None) -> list[dict]` (issue #636)
/// - `explain(quads) -> list[dict]` — cited-IRI explanation skeletons (issue #497)
/// - `certify(rules, profile) -> dict`
/// - `stable_models(rules, input) -> dict` — answer sets per world (issue #651)
/// - `query(world_nquads, query_program, profile, world_iri=None, max_answers=None, max_steps=None) -> dict`
///   (under `ProbabilisticProfile` each binding carries a `probability`; #506)
/// Compile a `logic:` RDF 1.2 source document (Turtle text) into all eight
/// committed artifacts, in Rust (issue #664).  The drop-in replacement for the
/// Python `logic_frontend` + `logic_projections` pipeline behind the registered
/// `LogicGenerator`.
///
/// Returns a dict keyed by artifact name (`owl_dl`, `owl_el`, `datalog`, `n3`,
/// `gufo`, `canonical_rdf12`, `nemo`, `report`), each mapping to the serialized
/// content string.  Text targets are byte-identical to the Python compiler; RDF
/// targets are RDF-isomorphic.  Raises `ValueError` on a parse failure, a Nemo
/// rule-safety violation, or an overclaim (Principle 7).
#[pyfunction]
fn compile_logic<'py>(py: Python<'py>, source_ttl: &str) -> PyResult<Bound<'py, PyDict>> {
    use crate::compile::frontend::parse_logic_str;
    use crate::compile::projections::compile_program;

    let (program, diagnostics) = parse_logic_str(source_ttl, None)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.0))?;
    let arts = compile_program(&program).map_err(pyo3::exceptions::PyValueError::new_err)?;

    // Build the canonical diagnostics Report in Rust (#856): the parse diagnostics
    // become `logic-compile.<code>` findings here, in the core, not via a Python
    // dict→finding reshaper. Normalize before handing it over so the live report
    // (and any downstream content hash / render) is deterministic — mirroring
    // `verify_native`.
    let diag_report = crate::compile::frontend::diagnostics_report(&diagnostics).normalized();

    let out = PyDict::new(py);
    out.set_item("owl_dl", arts.owl_dl)?;
    out.set_item("owl_el", arts.owl_el)?;
    out.set_item("datalog", arts.datalog)?;
    out.set_item("n3", arts.n3)?;
    out.set_item("gufo", arts.gufo)?;
    out.set_item("canonical_rdf12", arts.canonical_rdf12)?;
    out.set_item("nemo", arts.nemo)?;
    out.set_item("report", arts.report)?;
    // The reasoning-engine rule surface (the `% === Rules ===` section of the nemo
    // projection) — so the runner stops re-extracting it from `nemo` in Python.
    out.set_item("nemo_rules", arts.nemo_rules)?;
    // The preservation ledger as a JSON-able dict, keyed by target name, each value
    // `{preservation, complexity, lossy_drops}` — the exact shape the conformance
    // runner compares against `expected/projections/preservation-ledger.json`.
    let ledger = PyDict::new(py);
    for entry in &arts.preservation_ledger {
        let row = PyDict::new(py);
        row.set_item("preservation", entry.preservation.as_str())?;
        row.set_item("complexity", entry.complexity.as_str())?;
        let drops: Vec<&str> = entry.lossy_drops.iter().map(String::as_str).collect();
        row.set_item("lossy_drops", drops)?;
        ledger.set_item(entry.target.as_str(), row)?;
    }
    out.set_item("preservation_ledger", ledger)?;
    // The parse diagnostics as a live, normalized `gmeow_diagnostics` Report (#856),
    // not a `list[dict]`. The Python surface forwards it directly.
    out.set_item(
        "diagnostics_report",
        Py::new(
            py,
            gmeow_diagnostics::py::PyReport::from_engine(diag_report),
        )?,
    )?;
    Ok(out)
}

// ── build_divergence_ledger ───────────────────────────────────────────────────

/// Extract a `(subject, object, world)` triple from a Python sequence row.
///
/// Accepts any 3-element Python sequence (tuple/list) of strings. A row of the
/// wrong length or element type is a hard input error (`ValueError`) — a silently
/// dropped row would corrupt the divergence comparison.
fn extract_triple(row: &Bound<'_, PyAny>) -> PyResult<(String, String, String)> {
    let items: Vec<String> = row.extract().map_err(|_| {
        pyo3::exceptions::PyValueError::new_err(
            "subsumption row must be a sequence of 3 strings (subject, object, world)",
        )
    })?;
    if items.len() != 3 {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "subsumption row must have exactly 3 elements, got {}",
            items.len()
        )));
    }
    Ok((items[0].clone(), items[1].clone(), items[2].clone()))
}

/// Serialize one [`crate::reason::ledger::LedgerRow`] to a Python dict.
fn ledger_row_to_dict(
    py: Python<'_>,
    row: &crate::reason::ledger::LedgerRow,
) -> PyResult<Py<PyAny>> {
    use crate::reason::ledger::DivergenceKind;
    let kind = match row.kind {
        DivergenceKind::Agree => "Agree",
        DivergenceKind::NativeOnly => "NativeOnly",
        DivergenceKind::OracleOnly => "OracleOnly",
        DivergenceKind::DlGap => "DlGap",
    };
    let d = PyDict::new(py);
    d.set_item("kind", kind)?;
    d.set_item("category", &row.category)?;
    d.set_item("subject", &row.subject)?;
    d.set_item("object", &row.object)?;
    d.set_item("world", &row.world)?;
    d.set_item("detail", &row.detail)?;
    Ok(d.into_any().unbind())
}

/// Build the native↔oracle divergence ledger and classify every row (issue #666
/// Task 4 — the ENFORCED classic-cross-check lane).
///
/// This is the PyO3 surface over the authoritative Rust comparison logic in
/// [`crate::reason::ledger`]; it does NOT re-implement any comparison. Python owns
/// the Docker orchestration (running ELK/HermiT) and the enforcement decision;
/// this function owns the structured classification.
///
/// # Arguments
///
/// - `native_subsumptions` — native derived subsumptions, each a 3-element
///   sequence `(subject, object, world)`.
/// - `elk_subsumptions` — ELK-inferred subsumptions, same shape.
/// - `native_consistent` — the native DL consistency verdict.
/// - `native_unsat` — native unsatisfiable-class IRIs.
/// - `hermit_consistent` — HermiT's consistency verdict, or `None` if HermiT was
///   not run (recorded as a native-only note, never a divergence).
/// - `hermit_unsat` — HermiT's unsatisfiable-class IRIs.
/// - `gaps` — native DL coverage defects, each a 2-tuple `(code, message)`;
///   each becomes one failing `DlGap` row.
///
/// # Returns
///
/// A dict with keys:
/// - `rows` — `list[dict]`, each `{kind, category, subject, object, world, detail}`
///   where `kind` is one of `"Agree"`, `"NativeOnly"`, `"OracleOnly"`, `"DlGap"`.
/// - `agree`, `native_only`, `oracle_only`, `dl_gap` — per-kind tallies (int).
/// - `passed` (bool) — the **Rust-computed** strict enforcement verdict (#697
///   criterion 3): `True` only with zero `NativeOnly`/`OracleOnly`/`DlGap` rows.
///   This is the single decision authority; Python surfaces it unchanged and does
///   NOT recompute it.
/// - `reasons` — `list[str]`, one deterministic English reason per failing
///   category (empty when `passed`).
///
/// # Errors
///
/// Raises `ValueError` for a malformed subsumption or gap row.
#[pyfunction]
#[pyo3(signature = (
    native_subsumptions,
    elk_subsumptions,
    native_consistent,
    native_unsat,
    hermit_consistent,
    hermit_unsat,
    gaps
))]
#[allow(clippy::too_many_arguments)]
fn build_divergence_ledger(
    py: Python<'_>,
    native_subsumptions: Vec<Bound<'_, PyAny>>,
    elk_subsumptions: Vec<Bound<'_, PyAny>>,
    native_consistent: bool,
    native_unsat: Vec<String>,
    hermit_consistent: Option<bool>,
    hermit_unsat: Vec<String>,
    gaps: Vec<(String, String)>,
) -> PyResult<Py<PyAny>> {
    use crate::reason::ledger::{
        build_ledger, compare_consistency, compare_subsumption, dl_gap_rows, enforce,
    };

    let native: Vec<(String, String, String)> = native_subsumptions
        .iter()
        .map(extract_triple)
        .collect::<PyResult<_>>()?;
    let elk: Vec<(String, String, String)> = elk_subsumptions
        .iter()
        .map(extract_triple)
        .collect::<PyResult<_>>()?;

    let subsumption_rows = compare_subsumption(&native, &elk);
    let consistency_rows = compare_consistency(
        native_consistent,
        &native_unsat,
        hermit_consistent,
        &hermit_unsat,
    );
    // The ledger gap-row builder takes RdfLoss; mint one per (code, message).
    let gap_losses: Vec<gmeow_rdf::RdfLoss> = gaps
        .iter()
        .map(|(code, message)| gmeow_rdf::RdfLoss::new(code, message))
        .collect();
    let gap_rows = dl_gap_rows(&gap_losses);

    let ledger = build_ledger(subsumption_rows, consistency_rows, gap_rows);

    // The strict pass/fail DECISION is computed HERE, in Rust (#697 criterion 3) —
    // never in Python. The thin Python `classic_cross_check.enforce()` wrapper only
    // surfaces this `passed` flag.
    let verdict = enforce(&ledger);

    let rows = PyList::empty(py);
    for row in &ledger.rows {
        rows.append(ledger_row_to_dict(py, row)?)?;
    }

    let out = PyDict::new(py);
    out.set_item("rows", rows)?;
    out.set_item("agree", ledger.agree)?;
    out.set_item("native_only", ledger.native_only)?;
    out.set_item("oracle_only", ledger.oracle_only)?;
    out.set_item("dl_gap", ledger.dl_gap)?;
    out.set_item("passed", verdict.passed)?;
    out.set_item("reasons", verdict.reasons)?;
    Ok(out.into_any().unbind())
}

// ── reason_native ─────────────────────────────────────────────────────────────

/// Run native OWL-2 reasoning over a `gmeow.gts` bundle (issue #665).
///
/// Ingests the RDF-1.2-first GTS bundle through the concrete
/// [`gmeow_rdf::RdfDataset`] import path, then runs
/// the single-chase combined entry point [`crate::reason::reason_all`] — the EL
/// subsumption closure and the DL consistency verdict are derived from ONE Nemo
/// chase, never two.
///
/// # Arguments
///
/// - `gts_bytes` — the serialized `gmeow.gts` bundle bytes (segments allowed).
///
/// # Returns
///
/// A dict with keys:
/// - `consistent` (bool)
/// - `inferred` (`list[dict]`): each `{subject, predicate, object, world, is_edb, rule_name}`
///   (`rule_name` is `None` for asserted EDB axioms)
/// - `unsatisfiable_classes` (`list[dict]`): each `{class, world}`
/// - `inconsistencies` (`list[dict]`): each `{individual, world}`
/// - `coverage` (`dict`): `{present, decided, unsupported}` construct lists
/// - `gaps` (`list[dict]`): each `{code, message}` — native coverage defects
///
/// # Errors
///
/// Raises `ValueError` if the GTS bundle cannot be read, and `RuntimeError` if the
/// native reasoning run fails (chase parse/validate/evaluate/decode, or a quad-read
/// failure during the gap scan).
#[pyfunction]
fn reason_native(py: Python<'_>, gts_bytes: &[u8]) -> PyResult<Py<PyAny>> {
    // Import the bundle into the frozen IR inside the GIL-released closure
    // (moving the bytes in) so the detached work is self-contained. `reason_all`
    // runs the single chase here.
    // Distinguish the two failure modes per the docstring: a GTS read/parse
    // failure is a caller-input error (`ValueError`); a reasoning failure (chase
    // parse/validate/evaluate/decode, or a gap-scan quad-read) is a `RuntimeError`.
    enum ReasonNativeError {
        GtsRead(String),
        Reason(String),
    }
    let bytes = gts_bytes.to_vec();
    let read_result: Result<crate::reason::ReasonResult, ReasonNativeError> =
        py.detach(move || {
            let bundle = gmeow_rdf::import_gts_events(&bytes)
                .map_err(|e| ReasonNativeError::GtsRead(format!("GTS read error: {e}")))?;
            crate::reason::reason_all(bundle.dataset.as_ref()).map_err(ReasonNativeError::Reason)
        });
    let result = read_result.map_err(|e| match e {
        ReasonNativeError::GtsRead(m) => pyo3::exceptions::PyValueError::new_err(m),
        ReasonNativeError::Reason(m) => {
            pyo3::exceptions::PyRuntimeError::new_err(format!("reason error: {m}"))
        }
    })?;

    let out = PyDict::new(py);
    out.set_item("consistent", result.verdict.consistent)?;

    let inferred = PyList::empty(py);
    for ax in &result.inferred {
        let d = PyDict::new(py);
        d.set_item("subject", ax.subject.as_str())?;
        d.set_item("predicate", ax.predicate.as_str())?;
        d.set_item("object", ax.object.as_str())?;
        d.set_item("world", ax.world.as_str())?;
        d.set_item("is_edb", ax.is_edb)?;
        d.set_item("rule_name", ax.rule_name.as_deref())?;
        inferred.append(d)?;
    }
    out.set_item("inferred", inferred)?;

    let unsat = PyList::empty(py);
    for u in &result.verdict.unsatisfiable_classes {
        let d = PyDict::new(py);
        d.set_item("class", u.class.as_str())?;
        d.set_item("world", u.world.as_str())?;
        unsat.append(d)?;
    }
    out.set_item("unsatisfiable_classes", unsat)?;

    let inconsist = PyList::empty(py);
    for w in &result.verdict.inconsistencies {
        let d = PyDict::new(py);
        d.set_item("individual", w.individual.as_str())?;
        d.set_item("world", w.world.as_str())?;
        inconsist.append(d)?;
    }
    out.set_item("inconsistencies", inconsist)?;

    let coverage = PyDict::new(py);
    coverage.set_item("present", result.verdict.coverage.present.clone())?;
    coverage.set_item("decided", result.verdict.coverage.decided.clone())?;
    coverage.set_item("unsupported", result.verdict.coverage.unsupported.clone())?;
    out.set_item("coverage", coverage)?;

    let gaps = PyList::empty(py);
    for g in &result.verdict.gaps {
        let d = PyDict::new(py);
        d.set_item("code", g.code.as_str())?;
        d.set_item("message", g.message.as_str())?;
        gaps.append(d)?;
    }
    out.set_item("gaps", gaps)?;

    Ok(out.into_any().unbind())
}

// ── reason_native_artifacts ───────────────────────────────────────────────────

/// Reason a `gmeow.gts` bundle ONCE and emit all three native RDF 1.2 Turtle
/// artifacts (issue #666 Task 3).
///
/// This is the single-call replacement for the retired Python emitters
/// (`gmeow_tools.reason.build_inferred_closure_ttl` / `build_explanations_ttl` /
/// `build_dl_el_ledger_ttl`): it runs the native EL/DL reasoning lane
/// ([`crate::reason::reason_all`]) exactly once and serializes the three
/// committed artifacts via the gmeow-rdf RDF 1.2 Turtle emitter
/// ([`crate::reason::artifacts`]). Reasoning runs with the GIL released.
///
/// # Arguments
///
/// - `gts_bytes` — the serialized `gmeow.gts` bundle bytes (segments allowed).
/// - `merge` — when true, the inferred-closure artifact prepends the asserted
///   (told) graph so the document is the union of asserted and derived axioms
///   (the `--merge` mode). The explanations and ledger artifacts are unaffected.
///
/// # Returns
///
/// A dict with three string keys:
/// - `closure` — the told-vs-inferred inferred-closure Turtle.
/// - `explanations` — the per-axiom proof-skeleton Turtle.
/// - `ledger` — the native gap-zero DL/EL crosscheck ledger Turtle.
///
/// # Errors
///
/// Raises `ValueError` if the GTS bundle cannot be read, and `RuntimeError` if
/// the native reasoning run fails or a derived axiom is missing its rule name
/// (the no-optionality / honesty invariant — fabricated provenance is never
/// emitted).
#[pyfunction]
#[pyo3(signature = (gts_bytes, merge=false))]
fn reason_native_artifacts(py: Python<'_>, gts_bytes: &[u8], merge: bool) -> PyResult<Py<PyAny>> {
    use crate::reason::artifacts::{
        build_dl_el_ledger_ttl, build_explanations_ttl, build_inferred_closure_ttl,
    };

    // Distinguish the two failure modes (GTS read = ValueError, reasoning /
    // emission = RuntimeError) the same way `reason_native` does. The
    // The GTS import, reasoning, and three serializations all run inside one
    // GIL-released closure.
    enum ArtifactsError {
        GtsRead(String),
        Reason(String),
    }
    let bytes = gts_bytes.to_vec();
    let built: Result<(String, String, String), ArtifactsError> = py.detach(move || {
        let bundle = gmeow_rdf::import_gts_events(&bytes)
            .map_err(|e| ArtifactsError::GtsRead(format!("GTS read error: {e}")))?;
        let dataset = bundle.dataset.as_ref();
        let result = crate::reason::reason_all(dataset).map_err(ArtifactsError::Reason)?;

        let merge_store = if merge { Some(dataset) } else { None };
        let closure =
            build_inferred_closure_ttl(&result, merge_store).map_err(ArtifactsError::Reason)?;
        let explanations = build_explanations_ttl(&result).map_err(ArtifactsError::Reason)?;
        let ledger = build_dl_el_ledger_ttl(&result);
        Ok((closure, explanations, ledger))
    });
    let (closure, explanations, ledger) = built.map_err(|e| match e {
        ArtifactsError::GtsRead(m) => pyo3::exceptions::PyValueError::new_err(m),
        ArtifactsError::Reason(m) => {
            pyo3::exceptions::PyRuntimeError::new_err(format!("reason error: {m}"))
        }
    })?;

    let out = PyDict::new(py);
    out.set_item("closure", closure)?;
    out.set_item("explanations", explanations)?;
    out.set_item("ledger", ledger)?;
    Ok(out.into_any().unbind())
}

// ── rl_closure ─────────────────────────────────────────────────────────────────

/// Compute the OWL 2 RL/RDF deductive closure of an RDF graph (issue #666 Task 5).
///
/// This is the native, Docker/Java-free **primary** entailment authority that
/// replaces the `owlrl` deductive-closure baseline the conversion suites called.
/// The computation is RDF-1.2-first: every quad is encoded into the generic
/// 4-ary `triple(?s, ?p, ?o, ?w)` relation (predicate-as-DATA), the world axis
/// threads through unchanged, and the fixed OWL 2 RL rule set
/// ([`crate::reason::rl::RL_RULES`]) runs through the SAME Nemo chase the EL/DL
/// lane uses. The per-property ternary `materialize` seam *cannot* express RL's
/// property-quantifying meta-rules (the predicate is a Nemo symbol there), which
/// is why this surface uses the generic-triple encoding instead.
///
/// # Arguments
///
/// - `input` — the source graph as N-Quads (named-graph triples close in their
///   world) or N-Triples (default-graph triples close in a single sentinel
///   world). rdflib's `graph.serialize(format="nt")` / `format="nquads"` both
///   feed this directly.
///
/// # Returns
///
/// The full closure (asserted + derived) as an N-Triples string ([`rl_closure_nt`])
/// or a list of live `gmeow_rdf.Quad` objects ([`rl_closure_quads`]). Term
/// rendering — skolem-IRI → blank-node, literal display, de-dup and sort — happens
/// in Rust ([`crate::reason::rl::RlClosure::to_ntriples`]), so the reasoning path
/// crosses the FFI boundary exactly once (issue #630; the Python helper no longer
/// re-renders rows).
///
/// # Errors
///
/// Raises `ValueError` on an N-Quads/N-Triples parse error and `RuntimeError`
/// on a chase or decode failure.
///
/// Compute the OWL 2 RL closure of `input` (the shared core of the two surfaces).
fn compute_rl_closure(py: Python<'_>, input: &str) -> PyResult<crate::reason::rl::RlClosure> {
    // Parse the input as N-Quads (a superset of N-Triples — bare triples land in
    // the default graph) into the frozen IR, then close it through the
    // generic-triple RL chase with the GIL released.
    let bytes = input.as_bytes().to_vec();
    let closure: Result<crate::reason::rl::RlClosure, (bool, String)> = py.detach(move || {
        let dataset = gmeow_rdf::dataset_from_bytes(&bytes, RdfFormat::NQuads)
            .map_err(|e| (true, format!("N-Quads parse error: {e}")))?;
        crate::reason::rl::rl_closure(dataset.as_ref()).map_err(|e| (false, e))
    });
    closure.map_err(|(is_parse, msg)| {
        if is_parse {
            pyo3::exceptions::PyValueError::new_err(msg)
        } else {
            pyo3::exceptions::PyRuntimeError::new_err(format!("rl_closure error: {msg}"))
        }
    })
}

/// Compute the OWL 2 RL/RDF deductive closure and return it as an N-Triples string.
///
/// The native, Docker/Java-free **primary** entailment authority that replaces the
/// `owlrl` deductive-closure baseline (issue #666 Task 5). See the module note on
/// [`compute_rl_closure`]; the full closure is rendered to a byte-stable N-Triples
/// document in Rust. `input` is N-Quads or N-Triples (rdflib's
/// `graph.serialize(format="nt"|"nquads")` feeds it directly).
#[pyfunction]
fn rl_closure_nt(py: Python<'_>, input: &str) -> PyResult<String> {
    if input.trim().is_empty() {
        return Ok(String::new());
    }
    Ok(compute_rl_closure(py, input)?.to_ntriples())
}

/// Compute the OWL 2 RL/RDF deductive closure and return live `gmeow_rdf.Quad`s.
///
/// The structured twin of [`rl_closure_nt`]: the closure is rendered to N-Triples
/// in Rust and re-parsed (reusing oxigraph's lossless term parser) into a list of
/// `gmeow_rdf.Quad` objects, so an rdflib caller folds the closure straight back
/// into its graph with no intermediate Python-side N-Triples render/parse seam
/// (issue #630). Blank nodes round-trip as blank nodes; literals keep
/// datatype/language. All quads land in the default graph (the world axis is
/// flattened for the single-default-graph close the suites use).
#[pyfunction]
fn rl_closure_quads(py: Python<'_>, input: &str) -> PyResult<Vec<Py<PyAny>>> {
    if input.trim().is_empty() {
        return Ok(vec![]);
    }
    let nt = compute_rl_closure(py, input)?.to_ntriples();
    // Re-parse the rendered closure with oxigraph so the tricky literal/datatype
    // and blank-node grammar is decoded by the same engine that serialized it,
    // then hand each quad to Python as a native `gmeow_rdf.Quad` (#630).
    let quads = py
        .detach(move || -> Result<Vec<oxigraph::model::Quad>, String> {
            let store = Store::new().map_err(|e| format!("store creation failed: {e}"))?;
            store
                .load_from_reader(RdfFormat::NTriples, nt.as_bytes())
                .map_err(|e| format!("RL closure re-parse failed: {e}"))?;
            store
                .iter()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("RL closure store iteration failed: {e}"))
        })
        .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;

    let mut out: Vec<Py<PyAny>> = Vec::with_capacity(quads.len());
    for quad in &quads {
        out.push(gmeow_rdf::py_store::quad_to_py(py, quad)?);
    }
    Ok(out)
}

// ── verify_native ─────────────────────────────────────────────────────────────

/// Run the native reasoned-graph verify over a `gmeow.gts` bundle (issue #695).
///
/// Materializes the asserted graph (flattened) unioned with the native EL/DL
/// derived edges, runs each `(name, sparql)` SELECT query over it, and returns
/// the resulting diagnostics report as a **live `Report` pyclass** (not a JSON
/// string). All bindings now share one `Report` type in the `gmeow_native`
/// cdylib, so handing the live object back eliminates the serialize→`from_json`
/// round-trip the Python caller used to pay (#630, #695).
///
/// # Arguments
///
/// - `gts_bytes` — the serialized `gmeow.gts` bundle bytes (segments allowed).
/// - `queries` — `[(repo_relative_rq_path, sparql_text), …]`; discovery (incl.
///   slice verify queries) is the Python caller's job (repo-layout knowledge).
///
/// # Returns
///
/// The normalized [`gmeow_diagnostics::Report`] as a live pyclass. The report's
/// `ok` is false iff any verify query returned a row.
///
/// # Errors
///
/// Raises `ValueError` if the GTS bundle cannot be read, and `RuntimeError` if
/// verify fails (reasoning, a query parse/eval error, a non-SELECT query, or a
/// derived-edge build error).
#[pyfunction]
fn verify_native(
    py: Python<'_>,
    gts_bytes: &[u8],
    queries: Vec<(String, String)>,
) -> PyResult<Py<gmeow_diagnostics::py::PyReport>> {
    enum VerifyNativeError {
        GtsRead(String),
        Verify(String),
    }
    let bytes = gts_bytes.to_vec();
    let verify_result: Result<gmeow_diagnostics::Report, VerifyNativeError> =
        py.detach(move || {
            let bundle = gmeow_rdf::import_gts_events(&bytes)
                .map_err(|e| VerifyNativeError::GtsRead(format!("GTS read error: {e}")))?;
            crate::verify::verify(bundle.dataset.as_ref(), &queries)
                .map_err(VerifyNativeError::Verify)
        });
    let report = verify_result.map_err(|e| match e {
        VerifyNativeError::GtsRead(m) => pyo3::exceptions::PyValueError::new_err(m),
        VerifyNativeError::Verify(m) => {
            pyo3::exceptions::PyRuntimeError::new_err(format!("verify error: {m}"))
        }
    })?;
    // Normalize before handing it over so the live report (and any downstream
    // content hash / render) is deterministic, matching the diagnostics render
    // contract.
    Py::new(
        py,
        gmeow_diagnostics::py::PyReport::from_engine(report.normalized()),
    )
}

// ── extract_module ──────────────────────────────────────────────────────────────

/// Extract a syntactic-locality module (SLME) from a source ontology (issue #695).
///
/// Native, Java/Docker-free replacement for the ROBOT `extract` shell-out. Computes
/// a *module* of `ontology_ttl` around the seed signature `terms` using ⊥-/⊤-locality
/// (`method` ∈ `{"STAR", "BOT", "TOP"}`, case-insensitive; unknown → STAR with a
/// warning). The module is **sound, not necessarily minimal**: any construct that
/// touches the signature is kept, and constructs not classified by exact locality are
/// kept conservatively (with a `slme.conservative-keep` warning). It may therefore be
/// a superset of ROBOT's output — over-extraction is acceptable, under-extraction is
/// not.
///
/// # Arguments
///
/// - `ontology_ttl` — the source ontology as Turtle text.
/// - `terms` — the seed term IRIs (the signature Σ).
/// - `method` — `"STAR"` (default/unknown), `"BOT"`, or `"TOP"` (case-insensitive).
///
/// # Returns
///
/// A dict with keys:
/// - `module_ttl` (str) — the extracted module, deterministic Turtle.
/// - `selected_axiom_count` (int) — number of top-level (named-subject) kept triples.
/// - `method` (str) — the normalized method actually used.
/// - `warnings` (`list[dict]`) — each `{code, message}` from the conservative-keep /
///   unknown-method findings.
///
/// # Errors
///
/// Raises `ValueError` if the Turtle source cannot be parsed or the in-memory store
/// fails to build/iterate.
#[pyfunction]
fn extract_module(
    py: Python<'_>,
    ontology_ttl: &str,
    terms: Vec<String>,
    method: &str,
) -> PyResult<Py<PyAny>> {
    // Run the parse + extract with the GIL released; the closure returns the plain
    // data needed to build the Python dict afterwards (no Python objects cross the
    // detach boundary, mirroring reason_native).
    let ttl = ontology_ttl.to_owned();
    let method_owned = method.to_owned();
    let result = py
        .detach(move || crate::slme::extract_module(&ttl, &terms, &method_owned))
        .map_err(pyo3::exceptions::PyValueError::new_err)?;

    let warnings = PyList::empty(py);
    for finding in &result.findings {
        let w = PyDict::new(py);
        w.set_item("code", finding.code.as_str())?;
        w.set_item("message", finding.message.as_str())?;
        warnings.append(w)?;
    }

    let out = PyDict::new(py);
    out.set_item("module_ttl", result.module_ttl)?;
    out.set_item("selected_axiom_count", result.selected_axiom_count)?;
    out.set_item("method", result.method.as_str())?;
    out.set_item("warnings", warnings)?;
    Ok(out.into_any().unbind())
}

/// Register the `gmeow-logic` surface on a Python module.
///
/// Called by the unified `gmeow_native` cdylib (#630) to populate the
/// `gmeow_native.logic` submodule; the legacy `import gmeow_logic` name resolves
/// to that same submodule object via a Python shim.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(materialize, m)?)?;
    m.add_function(wrap_pyfunction!(materialize_explained, m)?)?;
    m.add_function(wrap_pyfunction!(foundation, m)?)?;
    m.add_function(wrap_pyfunction!(explain, m)?)?;
    m.add_function(wrap_pyfunction!(certify, m)?)?;
    m.add_function(wrap_pyfunction!(stable_models, m)?)?;
    m.add_function(wrap_pyfunction!(query, m)?)?;
    m.add_function(wrap_pyfunction!(compile_logic, m)?)?;
    m.add_function(wrap_pyfunction!(reason_native, m)?)?;
    m.add_function(wrap_pyfunction!(reason_native_artifacts, m)?)?;
    m.add_function(wrap_pyfunction!(rl_closure_nt, m)?)?;
    m.add_function(wrap_pyfunction!(rl_closure_quads, m)?)?;
    m.add_function(wrap_pyfunction!(build_divergence_ledger, m)?)?;
    m.add_function(wrap_pyfunction!(verify_native, m)?)?;
    m.add_function(wrap_pyfunction!(extract_module, m)?)?;
    Ok(())
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::decode_nemo_term;
    use crate::materialize::reifier_for_quad;
    use crate::provenance::term_n3;
    use oxigraph::model::Term;

    // ── reifier_for_quad ──────────────────────────────────────────────────────

    #[test]
    fn reifier_for_quad_golden_1() {
        // Matches golden-1 from determinism-goldens.json
        let s = Term::NamedNode(NamedNode::new("http://example.org/a").unwrap());
        let p = NamedNode::new("http://example.org/related").unwrap();
        let o = Term::NamedNode(NamedNode::new("http://example.org/b").unwrap());
        let got = reifier_for_quad(&s, &p, &o).expect("IRI terms must not fail");
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/reifier/10d9bdab72fe25cf3b81fe842b3a105077d98a6a"
        );
    }

    // ── term_n3 reexport from provenance ─────────────────────────────────────

    #[test]
    fn term_n3_iri_for_quad_object() {
        let nn = NamedNode::new("http://example.org/Foo").unwrap();
        let term = Term::NamedNode(nn);
        assert_eq!(
            term_n3(&term).expect("IRI term must not fail"),
            "<http://example.org/Foo>"
        );
    }

    // ── decode_nemo_term imported from encode ─────────────────────────────────

    #[test]
    fn py_decode_nemo_term_iri_smoke() {
        // Verify that py.rs can use decode_nemo_term from crate::encode.
        let term = decode_nemo_term("<http://example.org/Smoke>").unwrap();
        match term {
            Term::NamedNode(nn) => assert_eq!(nn.as_str(), "http://example.org/Smoke"),
            other => panic!("expected NamedNode, got {other:?}"),
        }
    }
}
